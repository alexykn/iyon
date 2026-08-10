//! Markdown rendering for assistant text.
//!
//! Turns `AssistantSegment`s into a **shared, width-independent semantic
//! document** (`AssistantDocument`), consumed by the finalized View path and
//! the specialized AssistantStream provenance path:
//!
//! ```text
//!                 AssistantDocument
//!                         |
//!                 finalized semantic View
//! ```
//!
//! There is exactly **one** Markdown interpretation. Any fix to bold/italic/
//! code/heading/list classification/nesting/Thinking styling happens in
//! `parse_assistant` before those consumers diverge.
//!
//! The document holds no backend or terminal types. `AssistantLogicalRow` carries:
//!
//! * `spans` — semantic `TextSpan`s (appearance intent);
//! * `layout` — plain vs list-item (structure/geometry intent);
//! * `style` — the row-level gutter/prefix style;
//! * `source` — source stability / streaming bookkeeping (`content_len`,
//!   `has_newline`, `stable_prefix_len`), used only by the
//!   still-special HostedStream provenance path.
//!
//! Streaming is *tolerant*: an unclosed marker (`**unclosed`, `*ital`, `` `code ``)
//! renders literally rather than being suppressed. A marker only becomes styled
//! once its closer actually arrives, keeping streaming safe.
//!
//! The inline parser is **extended-grapheme safe and never panics on any valid
//! UTF-8 prefix** (a live stream feeds partial source). Every offset used to
//! slice `&str` is a byte offset derived from char/byte boundaries.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::presentation::{
    ColorSpec, Insets, IntoView, StyleSpec, TextAttributeSpec, TextSpan, ThemeKey, View,
};
use crate::transcript::model::{AssistantSegment, SegmentKind};

// ---------------------------------------------------------------------------
// Width-independent assistant document
// ---------------------------------------------------------------------------

/// Width-independent interpretation of `AssistantSegment` source.
#[derive(Debug, Clone)]
pub(crate) struct AssistantDocument {
    pub(crate) rows: Vec<AssistantLogicalRow>,
}

/// One logical row of the assistant document: semantic appearance + geometry
/// intent, plus the streaming source sidecar.
#[derive(Debug, Clone)]
pub(crate) struct AssistantLogicalRow {
    /// Inline content as semantic spans.
    pub(crate) spans: Vec<TextSpan>,
    /// Source-mapped inline display runs used by streaming presentation.
    pub(crate) projected_runs: Vec<AssistantProjectedRun>,
    /// Structural/geometry intent (plain vs list item).
    pub(crate) layout: AssistantRowLayout,
    /// Row-level style for semantic list markers and heading prefixes.
    pub(crate) style: StyleSpec,
    /// Source stability metadata for the specialized streaming path.
    pub(crate) source: AssistantSourceMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AssistantRowLayout {
    Plain,
    Heading,
    ListContinuation {
        body_column: u16,
    },
    ListItem {
        depth: usize,
        marker: AssistantMarker,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssistantMarker {
    Bullet,
    Ordered { index: usize },
}

/// Parser continuation context for the first logical row when live stream source is compacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssistantContinuation {
    Paragraph,
    Heading,
    List { body_column: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AssistantProjectedRun {
    pub(crate) display: String,
    pub(crate) style: StyleSpec,
    /// Row-relative source bytes owned by this displayed run.
    pub(crate) owned: Range<usize>,
    /// Visible source bytes, when the display is an exact source projection.
    pub(crate) exact_visible: Option<Range<usize>>,
    /// Context needed when restarting inside the visible portion. `None` means
    /// the run can be reparsed from any legal checkpoint within that portion.
    pub(crate) restart_from: Option<usize>,
    /// Context needed when restarting in hidden source attached before the
    /// visible portion.
    pub(crate) prefix_restart_from: Option<usize>,
}

/// Source stability / streaming bookkeeping. Neither contains terminal width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssistantSourceMeta {
    /// Source byte length of this logical row's line (excluding trailing `\n`).
    pub(crate) content_len: usize,
    /// Whether this logical row terminates in a hard newline `\n`.
    pub(crate) has_newline: bool,
    /// Largest source-byte prefix of THIS logical row whose current presentation
    /// cannot change under any future append, including semantic reinterpretation
    /// and extended-grapheme safety. Range: 0..=content_len.
    pub(crate) stable_prefix_len: usize,
}

impl AssistantSourceMeta {
    pub(crate) fn total_len(&self) -> usize {
        self.content_len + if self.has_newline { 1 } else { 0 }
    }
}

/// Parse `segments` once into the shared width-independent document.
pub(crate) fn parse_assistant(segments: &[AssistantSegment]) -> AssistantDocument {
    parse_assistant_tail(segments, crate::stream::StreamOffset::ZERO, None)
}

/// Parse `segments` into the shared document starting at `source_base`, with an optional
/// continuation mode for the first logical row.
pub(crate) fn parse_assistant_tail(
    segments: &[AssistantSegment],
    _source_base: crate::stream::StreamOffset,
    continuation: Option<AssistantContinuation>,
) -> AssistantDocument {
    let raw_lines = flatten_lines(segments);
    let mut rows = Vec::with_capacity(raw_lines.len());
    for (i, line) in raw_lines.into_iter().enumerate() {
        if i == 0 {
            if let Some(cont) = continuation {
                rows.push(parse_continuation_line(line, cont));
                continue;
            }
        }
        rows.push(parse_line(line));
    }
    AssistantDocument { rows }
}

// ---------------------------------------------------------------------------
// Logical-line flattening
// ---------------------------------------------------------------------------

/// One logical line of the stream: (kind, text, absolute source byte offset).
#[derive(Debug)]
struct RawLine {
    pieces: Vec<(SegmentKind, String, usize)>,
    has_newline: bool,
}

/// Flattens segments into logical lines, tracking absolute source byte offsets
/// (relative to the start of the concatenated, non-frozen text).
fn flatten_lines(segments: &[AssistantSegment]) -> Vec<RawLine> {
    let mut lines: Vec<RawLine> = Vec::new();
    let mut cur: Vec<(SegmentKind, String, usize)> = Vec::new();
    let mut abs: usize = 0;

    for segment in segments {
        let kind = match segment {
            AssistantSegment::Text(_) => SegmentKind::Text,
            AssistantSegment::Thinking(_) => SegmentKind::Thinking,
        };
        let text = segment.text();
        for (idx, piece) in text.split('\n').enumerate() {
            if idx > 0 {
                lines.push(RawLine {
                    pieces: std::mem::take(&mut cur),
                    has_newline: true,
                });
                abs += '\n'.len_utf8();
            }
            if !piece.is_empty() {
                cur.push((kind, piece.to_string(), abs));
                abs += piece.len();
            }
        }
    }

    if !cur.is_empty() || lines.is_empty() {
        lines.push(RawLine {
            pieces: cur,
            has_newline: false,
        });
    }
    lines
}

// ---------------------------------------------------------------------------
// Document parse: assign semantic spans, layout, style, and source metadata.
// ---------------------------------------------------------------------------

/// Largest source-byte prefix whose final extended grapheme cluster cannot be
/// changed by a future append. The final open grapheme is deliberately held
/// back, even when the current semantic interpretation is otherwise pinned.
fn open_egc_stable_prefix_len(source: &str) -> usize {
    source
        .grapheme_indices(true)
        .map(|(offset, _)| offset)
        .last()
        .unwrap_or(0)
}

/// Intersects semantic append stability with source extended-grapheme safety.
fn open_stable_prefix_len(source: &str, semantic_prefix_len: usize) -> usize {
    semantic_prefix_len.min(open_egc_stable_prefix_len(source))
}

fn parse_continuation_line(line: RawLine, cont: AssistantContinuation) -> AssistantLogicalRow {
    let full = concat_pieces(&line);
    let has_newline = line.has_newline;
    let content_len = piece_source_len(&line);

    if full.is_empty() {
        return AssistantLogicalRow {
            spans: Vec::new(),
            projected_runs: Vec::new(),
            layout: AssistantRowLayout::Plain,
            style: StyleSpec::default(),
            source: AssistantSourceMeta {
                content_len: 0,
                has_newline,
                stable_prefix_len: 0,
            },
        };
    }

    match cont {
        AssistantContinuation::Heading => {
            let semantic_prefix_len = content_len;
            let stable_prefix_len = if has_newline {
                content_len
            } else {
                open_stable_prefix_len(&full, semantic_prefix_len)
            };
            AssistantLogicalRow {
                spans: header_spans(&line),
                projected_runs: Vec::new(),
                layout: AssistantRowLayout::Heading,
                style: header_style(),
                source: AssistantSourceMeta {
                    content_len,
                    has_newline,
                    stable_prefix_len,
                },
            }
        }
        AssistantContinuation::Paragraph => {
            let (spans, projected_runs, _restricted, paragraph_semantic_stable) =
                paragraph_projected(&line);
            let stable_prefix_len = if has_newline {
                content_len
            } else {
                open_stable_prefix_len(&full, paragraph_semantic_stable)
            };
            AssistantLogicalRow {
                spans,
                projected_runs,
                layout: AssistantRowLayout::Plain,
                style: StyleSpec::default(),
                source: AssistantSourceMeta {
                    content_len,
                    has_newline,
                    stable_prefix_len,
                },
            }
        }
        AssistantContinuation::List { body_column } => {
            let (spans, projected_runs, body_semantic_stable) = body_inline(&full);
            AssistantLogicalRow {
                spans,
                projected_runs,
                layout: AssistantRowLayout::ListContinuation { body_column },
                style: list_style(),
                source: AssistantSourceMeta {
                    content_len,
                    has_newline,
                    stable_prefix_len: if has_newline {
                        content_len
                    } else {
                        open_stable_prefix_len(&full, body_semantic_stable)
                    },
                },
            }
        }
    }
}

/// True when the open prefix of a line is ambiguous between plain text and a heading/list marker.
fn classification_is_ambiguous(full: &str) -> bool {
    let indent = list_indent(full);
    let trimmed = indent.trimmed_source;

    // A non-empty indentation-only prefix can still become a list when more
    // source arrives (e.g. "  " followed by "- item").
    if trimmed.is_empty() {
        return true;
    }

    // 1. Ambiguous heading: 1..=6 '#' with nothing after
    let hash_count = trimmed.chars().take_while(|c| *c == '#').count();
    if hash_count >= 1 && hash_count <= 6 && trimmed[hash_count..].is_empty() {
        return true;
    }

    // 2. Ambiguous unordered list marker: '-', '+', '*' alone
    if trimmed == "-" || trimmed == "+" || trimmed == "*" {
        return true;
    }

    // 3. Ambiguous ordered list prefix: digits alone, or digits followed by '.' or ')' with nothing after
    let digit_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count > 0 {
        let rest = &trimmed[digit_count..];
        if rest.is_empty() || rest == "." || rest == ")" {
            return true;
        }
    }

    false
}

fn parse_line(line: RawLine) -> AssistantLogicalRow {
    let full = concat_pieces(&line);
    let has_newline = line.has_newline;
    let content_len = piece_source_len(&line);

    if full.is_empty() {
        return AssistantLogicalRow {
            spans: Vec::new(),
            projected_runs: Vec::new(),
            layout: AssistantRowLayout::Plain,
            style: StyleSpec::default(),
            source: AssistantSourceMeta {
                content_len: 0,
                has_newline,
                stable_prefix_len: 0,
            },
        };
    }

    if !has_newline && classification_is_ambiguous(&full) {
        return AssistantLogicalRow {
            spans: paragraph_spans(&line).0,
            projected_runs: paragraph_projected(&line).1,
            layout: AssistantRowLayout::Plain,
            style: StyleSpec::default(),
            source: AssistantSourceMeta {
                content_len,
                has_newline,
                stable_prefix_len: open_stable_prefix_len(&full, 0),
            },
        };
    }

    if let Some(_count) = header_run(&full) {
        let semantic_prefix_len = content_len;
        let stable_prefix_len = if has_newline {
            content_len
        } else {
            open_stable_prefix_len(&full, semantic_prefix_len)
        };
        return AssistantLogicalRow {
            spans: header_spans(&line),
            projected_runs: Vec::new(),
            layout: AssistantRowLayout::Heading,
            style: header_style(),
            source: AssistantSourceMeta {
                content_len,
                has_newline,
                stable_prefix_len,
            },
        };
    }

    if let Some((depth, index, body_start, body_source)) = ordered_parts(&full) {
        let (spans, projected_runs, body_semantic_stable) = body_inline(body_source);
        let projected_runs = shift_projected_runs(projected_runs, body_start);
        let semantic_prefix_len = body_start.saturating_add(body_semantic_stable);
        let stable_prefix_len = if has_newline {
            content_len
        } else {
            open_stable_prefix_len(&full, semantic_prefix_len)
        };
        return AssistantLogicalRow {
            spans,
            projected_runs,
            layout: AssistantRowLayout::ListItem {
                depth,
                marker: AssistantMarker::Ordered { index },
            },
            style: list_style(),
            source: AssistantSourceMeta {
                content_len,
                has_newline,
                stable_prefix_len,
            },
        };
    }

    if let Some((depth, body_start, body_source)) = unordered_parts(&full) {
        let (spans, projected_runs, body_semantic_stable) = body_inline(body_source);
        let projected_runs = shift_projected_runs(projected_runs, body_start);
        let semantic_prefix_len = body_start.saturating_add(body_semantic_stable);
        let stable_prefix_len = if has_newline {
            content_len
        } else {
            open_stable_prefix_len(&full, semantic_prefix_len)
        };
        return AssistantLogicalRow {
            spans,
            projected_runs,
            layout: AssistantRowLayout::ListItem {
                depth,
                marker: AssistantMarker::Bullet,
            },
            style: list_style(),
            source: AssistantSourceMeta {
                content_len,
                has_newline,
                stable_prefix_len,
            },
        };
    }

    // Plain paragraph (may include Thinking pieces).
    let (spans, projected_runs, _restricted, paragraph_semantic_stable) =
        paragraph_projected(&line);
    let stable_prefix_len = if has_newline {
        content_len
    } else {
        open_stable_prefix_len(&full, paragraph_semantic_stable)
    };
    AssistantLogicalRow {
        spans,
        projected_runs,
        layout: AssistantRowLayout::Plain,
        style: StyleSpec::default(),
        source: AssistantSourceMeta {
            content_len,
            has_newline,
            stable_prefix_len,
        },
    }
}

// ---------------------------------------------------------------------------
// Semantic styles
// ---------------------------------------------------------------------------

fn attr(bold: bool, italic: bool) -> TextAttributeSpec {
    TextAttributeSpec {
        bold: bold.then_some(true),
        italic: italic.then_some(true),
        ..TextAttributeSpec::default()
    }
}

fn themed_foreground(key: &str, attributes: TextAttributeSpec) -> StyleSpec {
    StyleSpec {
        foreground: Some(ColorSpec::Theme(ThemeKey::from(key))),
        attributes,
        ..StyleSpec::default()
    }
}

pub(crate) fn thinking_style_spec() -> StyleSpec {
    themed_foreground("text.muted", attr(false, true))
}
fn header_style() -> StyleSpec {
    themed_foreground("markdown.header", attr(false, false))
}
fn list_style() -> StyleSpec {
    themed_foreground("markdown.list", attr(false, false))
}

// ---------------------------------------------------------------------------
// Semantic span / body construction
// ---------------------------------------------------------------------------

/// Header row spans: Thinking pieces stay muted+italic; Text pieces are the
/// tinted header accent.
fn header_spans(line: &RawLine) -> Vec<TextSpan> {
    line.pieces
        .iter()
        .map(|(kind, text, _)| {
            if *kind == SegmentKind::Thinking {
                TextSpan::styled(text.clone(), thinking_style_spec())
            } else {
                TextSpan::styled(text.clone(), header_style())
            }
        })
        .collect()
}

/// List-item body spans parsed for inline emphasis (base = plain). The returned
/// stability offset is relative to the raw body source passed in.
fn shift_projected_runs(
    mut runs: Vec<AssistantProjectedRun>,
    offset: usize,
) -> Vec<AssistantProjectedRun> {
    for run in &mut runs {
        run.owned.start += offset;
        run.owned.end += offset;
        if let Some(visible) = &mut run.exact_visible {
            visible.start += offset;
            visible.end += offset;
        }
        run.restart_from = run.restart_from.map(|restart| restart + offset);
        run.prefix_restart_from = run.prefix_restart_from.map(|restart| restart + offset);
    }
    runs
}

fn body_inline(body_source: &str) -> (Vec<TextSpan>, Vec<AssistantProjectedRun>, usize) {
    let (mut spans, mut runs, _restricted, stable_len) =
        parse_inline_projected(body_source, StyleSpec::default());
    normalize_list_display_tabs(&mut spans);
    runs = split_tab_replacements(runs);
    (spans, runs, stable_len)
}

/// Split tab expansion into source-local replacement atoms without making the
/// surrounding exact text indivisible.
fn split_tab_replacements(runs: Vec<AssistantProjectedRun>) -> Vec<AssistantProjectedRun> {
    let mut split = Vec::new();
    for run in runs {
        let Some(visible) = run.exact_visible.clone() else {
            split.push(run);
            continue;
        };
        if !run.display.contains('\t') {
            split.push(run);
            continue;
        }

        let mut pieces = Vec::new();
        let mut cursor = 0;
        for (tab_start, _) in run.display.match_indices('\t') {
            if tab_start > cursor {
                pieces.push((cursor..tab_start, true));
            }
            pieces.push((tab_start..tab_start + 1, false));
            cursor = tab_start + 1;
        }
        if cursor < run.display.len() {
            pieces.push((cursor..run.display.len(), true));
        }

        for (index, (display_range, exact)) in pieces.iter().enumerate() {
            let source_start = visible.start + display_range.start;
            let source_end = visible.start + display_range.end;
            let owned_start = if index == 0 {
                run.owned.start
            } else {
                source_start
            };
            let owned_end = if index + 1 == pieces.len() {
                run.owned.end
            } else {
                source_end
            };
            let display = if *exact {
                run.display[display_range.clone()].to_string()
            } else {
                "    ".to_string()
            };
            split.push(AssistantProjectedRun {
                display,
                style: run.style.clone(),
                owned: owned_start..owned_end,
                exact_visible: exact.then_some(source_start..source_end),
                restart_from: run.restart_from,
                prefix_restart_from: if index == 0 {
                    run.prefix_restart_from
                } else {
                    None
                },
            });
        }
    }
    split
}

/// Paragraph spans: Thinking pieces pass through as muted+italic; Text pieces
/// get inline emphasis. Returns whether any inline marker was hidden and the
/// semantic stable prefix byte length.
fn paragraph_projected(line: &RawLine) -> (Vec<TextSpan>, Vec<AssistantProjectedRun>, bool, usize) {
    let row_base = line.pieces.first().map_or(0, |(_, _, start)| *start);
    let mut spans = Vec::new();
    let mut runs = Vec::new();
    let mut restricted = false;
    let mut stable_prefix_len = 0;
    let mut hit_unstable = false;

    for (kind, text, source_start) in &line.pieces {
        if *kind == SegmentKind::Thinking {
            spans.push(TextSpan::styled(text.clone(), thinking_style_spec()));
            runs.push(AssistantProjectedRun {
                display: text.clone(),
                style: thinking_style_spec(),
                owned: source_start - row_base..source_start - row_base + text.len(),
                exact_visible: Some(source_start - row_base..source_start - row_base + text.len()),
                restart_from: None,
                prefix_restart_from: None,
            });
            if !hit_unstable {
                let piece_start = source_start - row_base;
                stable_prefix_len = piece_start + text.len();
            }
            continue;
        }

        let (parsed, mut parsed_runs, is_restricted, piece_stable) =
            parse_inline_projected(text, StyleSpec::default());
        for run in &mut parsed_runs {
            run.owned.start += source_start - row_base;
            run.owned.end += source_start - row_base;
            if let Some(visible) = &mut run.exact_visible {
                visible.start += source_start - row_base;
                visible.end += source_start - row_base;
            }
            run.restart_from = run
                .restart_from
                .map(|restart| restart + source_start - row_base);
            run.prefix_restart_from = run
                .prefix_restart_from
                .map(|restart| restart + source_start - row_base);
        }
        restricted |= is_restricted;
        spans.extend(parsed);
        runs.extend(parsed_runs);

        if !hit_unstable {
            let piece_start = source_start - row_base;
            stable_prefix_len = piece_start + piece_stable;
            if piece_stable < text.len() {
                hit_unstable = true;
            }
        }
    }
    (spans, runs, restricted, stable_prefix_len)
}

fn paragraph_spans(line: &RawLine) -> (Vec<TextSpan>, bool, usize) {
    let (spans, _runs, restricted, stable) = paragraph_projected(line);
    (spans, restricted, stable)
}

// ---------------------------------------------------------------------------
// Detection helpers
// ---------------------------------------------------------------------------

/// True when a line starts with a heading run (`#`..`######` followed by a
/// space or end of line).
fn header_run(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let count = trimmed.chars().take_while(|c| *c == '#').count();
    if count == 0 || count > 6 {
        return None;
    }
    let after = &trimmed[count..];
    (after.is_empty() || after.starts_with(' ')).then_some(count)
}

/// Leading indentation geometry and its corresponding raw source position.
/// Tabs count as four display columns, but the returned slice and offsets remain
/// in the original source coordinate space.
#[derive(Debug)]
struct ListIndent<'a> {
    depth: usize,
    source_start: usize,
    trimmed_source: &'a str,
}

fn list_indent(line: &str) -> ListIndent<'_> {
    let mut columns = 0usize;
    let mut source_start = 0usize;

    for (byte, ch) in line.char_indices() {
        match ch {
            ' ' => {
                columns += 1;
                source_start = byte + ch.len_utf8();
            }
            '\t' => {
                columns += 4;
                source_start = byte + ch.len_utf8();
            }
            _ => break,
        }
    }

    ListIndent {
        depth: columns / ASSISTANT_LIST_INDENT,
        source_start,
        trimmed_source: &line[source_start..],
    }
}

/// The length of an unordered marker `- ` / `* ` / `+ ` (symbol + space).
fn unordered_marker_len(trimmed_source: &str) -> Option<usize> {
    let first = trimmed_source.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        let mut chars = trimmed_source.chars();
        chars.next();
        if chars.next() == Some(' ') {
            return Some(first.len_utf8() + 1);
        }
    }
    None
}

/// Returns the raw source depth, body offset, and body slice for an unordered item.
fn unordered_parts(full: &str) -> Option<(usize, usize, &str)> {
    let indent = list_indent(full);
    let marker_len = unordered_marker_len(indent.trimmed_source)?;
    let body_start = indent.source_start + marker_len;
    Some((indent.depth, body_start, &full[body_start..]))
}

/// Expands tabs only in already-parsed list display spans. Source bookkeeping
/// must always use the raw source passed to the parser above.
fn normalize_list_display_tabs(spans: &mut [TextSpan]) {
    for span in spans {
        if span.text.contains('\t') {
            span.text = span.text.replace('\t', "    ");
        }
    }
}

/// Separator length of an ordered item (`1. ` / `1) `) including trailing space.
fn ordered_separator_len(trimmed: &str) -> Option<usize> {
    let mut digit_end = 0usize;
    for (i, c) in trimmed.char_indices() {
        if c.is_ascii_digit() {
            digit_end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if digit_end == 0 {
        return None;
    }
    let rest = &trimmed[digit_end..];
    let sep_len = if let Some(c) = rest.chars().next() {
        if c == '.' || c == ')' {
            c.len_utf8()
        } else {
            return None;
        }
    } else {
        return None;
    };
    let after = &rest[sep_len..];
    if after.is_empty() {
        Some(digit_end + sep_len)
    } else if after.starts_with(' ') {
        Some(digit_end + sep_len + 1)
    } else {
        None
    }
}

/// Returns the raw source depth, parsed index, body offset, and body slice for
/// an ordered item.
fn ordered_parts(full: &str) -> Option<(usize, usize, usize, &str)> {
    let indent = list_indent(full);
    let separator_len = ordered_separator_len(indent.trimmed_source)?;
    let index = ordered_index(indent.trimmed_source);
    let body_start = indent.source_start + separator_len;
    Some((indent.depth, index, body_start, &full[body_start..]))
}

/// Parses the numeric index of an ordered item.
fn ordered_index(trimmed: &str) -> usize {
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Inline emphasis
//
// A small recursive scanner. `**bold**`, `*italic*`, `_italic_`, `` `code` ``, and
// `***bold+italic***` are recognized, and emphasis nests (`**b _i_ b**`,
// `*b **i** b*`). A delimiter run surrounded by whitespace on both sides is
// literal (so `2 * 3` is not italicized). Unclosed markers render literally so
// streaming stays safe. Every index used to slice `&str` is a byte offset; the
// parser never panics on any valid UTF-8 source/prefix.
// ---------------------------------------------------------------------------

/// The emphasis strength of an opener run.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Emphasis {
    Italic,
    Bold,
    BoldItalic,
}

#[derive(Debug, Clone)]
enum InlinePiece {
    Visible {
        text: String,
        style: StyleSpec,
        source: Range<usize>,
        restart_from: Option<usize>,
    },
    Hidden {
        source: Range<usize>,
        restart_from: Option<usize>,
    },
}

/// Adds the emphasis strength's attribute flags onto `base` (foreground kept, so
/// nested emphasis inherits the outer color while stacking modifiers).
fn combine_style(base: &StyleSpec, kind: Emphasis) -> StyleSpec {
    let mut style = base.clone();
    match kind {
        Emphasis::Italic => style.attributes.italic = Some(true),
        Emphasis::Bold => style.attributes.bold = Some(true),
        Emphasis::BoldItalic => {
            style.attributes.bold = Some(true);
            style.attributes.italic = Some(true);
        }
    }
    style
}

/// True when a delimiter run at byte offset `i` is flanked by whitespace / string
/// edges on both sides. Such detached delimiters never open or close emphasis
/// (`2 * 3`).
fn is_detached(text: &str, i: usize, run_bytes: usize) -> bool {
    debug_assert!(text.is_char_boundary(i));
    let before = text[..i].chars().next_back();
    let after = if i + run_bytes <= text.len() {
        debug_assert!(text.is_char_boundary(i + run_bytes));
        text[i + run_bytes..].chars().next()
    } else {
        None
    };
    let left_ws = before.map_or(true, |c| c.is_whitespace());
    let right_ws = after.map_or(true, |c| c.is_whitespace());
    left_ws && right_ws
}

/// Finds the closing delimiter for an opener: the first run of exactly `need_count`
/// delimiters at or after byte `from`. Runs of a different length are nested
/// emphasis and are skipped (consumed into the inner content). Returns the
/// closer's start byte offset and its run byte length.
fn find_closer(text: &str, from: usize, ch: char, need_count: usize) -> Option<(usize, usize)> {
    debug_assert!(text.is_char_boundary(from));
    let mut j = from;
    while j < text.len() {
        debug_assert!(text.is_char_boundary(j));
        let c = text[j..].chars().next()?;
        if c != ch {
            j += c.len_utf8();
            continue;
        }
        let run_count = text[j..].chars().take_while(|c0| *c0 == ch).count();
        let run_bytes = run_count * ch.len_utf8();
        if run_count == need_count {
            return Some((j, run_bytes));
        }
        // ch is a 1-byte ASCII delimiter, so `run` delimiters occupy `run_bytes` bytes.
        j += run_bytes;
    }
    None
}

fn parse_inline_rec(
    text: &str,
    base: StyleSpec,
    source_base: usize,
    active_restart: Option<usize>,
) -> (Vec<InlinePiece>, bool, usize) {
    let mut out: Vec<InlinePiece> = Vec::new();
    let mut plain = String::new();
    let mut plain_start = 0usize;
    let mut restricted = false;
    let mut unstable_from: Option<usize> = None;
    let mut i = 0usize;

    macro_rules! begin_plain {
        () => {
            if plain.is_empty() {
                plain_start = i;
            }
        };
    }
    macro_rules! flush {
        () => {
            if !plain.is_empty() {
                out.push(InlinePiece::Visible {
                    text: std::mem::take(&mut plain),
                    style: base.clone(),
                    source: source_base + plain_start..source_base + i,
                    restart_from: active_restart,
                });
            }
        };
    }

    let record_unstable = |unstable_from: &mut Option<usize>, offset: usize| {
        if let Some(existing) = *unstable_from {
            *unstable_from = Some(existing.min(offset));
        } else {
            *unstable_from = Some(offset);
        }
    };

    while i < text.len() {
        debug_assert!(text.is_char_boundary(i));
        let ch = text[i..].chars().next().expect("valid byte index");
        if ch == '`' {
            let rest = &text[i + 1..];
            if let Some(rel) = rest.find('`') {
                flush!();
                let mut code_style = base.clone();
                code_style.foreground = Some(ColorSpec::Theme(ThemeKey::from("markdown.code")));
                let construct_restart = active_restart.or(Some(source_base + i));
                out.push(InlinePiece::Hidden {
                    source: source_base + i..source_base + i + 1,
                    restart_from: construct_restart,
                });
                out.push(InlinePiece::Visible {
                    text: rest[..rel].to_string(),
                    style: code_style,
                    source: source_base + i + 1..source_base + i + 1 + rel,
                    restart_from: construct_restart,
                });
                out.push(InlinePiece::Hidden {
                    source: source_base + i + 1 + rel..source_base + i + rel + 2,
                    restart_from: construct_restart,
                });
                restricted = true;
                i += rel + 2;
            } else {
                record_unstable(&mut unstable_from, i);
                begin_plain!();
                plain.push(ch);
                i += 1;
            }
            continue;
        }

        if ch == '*' || ch == '_' {
            let run_count = text[i..].chars().take_while(|c0| *c0 == ch).count();
            let run_bytes = run_count * ch.len_utf8();
            if i + run_bytes == text.len() {
                record_unstable(&mut unstable_from, i);
            }

            if is_detached(text, i, run_bytes) {
                begin_plain!();
                for _ in 0..run_count {
                    plain.push(ch);
                }
                i += run_bytes;
                continue;
            }
            let (kind, open_count) = if run_count >= 3 {
                (Emphasis::BoldItalic, 3)
            } else if run_count == 2 {
                (Emphasis::Bold, 2)
            } else {
                (Emphasis::Italic, 1)
            };
            let open_bytes = open_count * ch.len_utf8();
            if let Some((close_start, close_bytes)) =
                find_closer(text, i + open_bytes, ch, open_count)
            {
                flush!();
                let construct_restart = active_restart.or(Some(source_base + i));
                out.push(InlinePiece::Hidden {
                    source: source_base + i..source_base + i + open_bytes,
                    restart_from: construct_restart,
                });
                let inner = &text[i + open_bytes..close_start];
                let inner_base = combine_style(&base, kind);
                let (inner_pieces, _inner_restricted, inner_stable) = parse_inline_rec(
                    inner,
                    inner_base,
                    source_base + i + open_bytes,
                    construct_restart,
                );
                out.extend(inner_pieces);
                if inner_stable < inner.len() {
                    record_unstable(&mut unstable_from, i + open_bytes + inner_stable);
                }
                out.push(InlinePiece::Hidden {
                    source: source_base + close_start..source_base + close_start + close_bytes,
                    restart_from: construct_restart,
                });
                restricted = true;
                if close_start + close_bytes == text.len() {
                    record_unstable(&mut unstable_from, i);
                }
                i = close_start + close_bytes;
            } else {
                record_unstable(&mut unstable_from, i);
                begin_plain!();
                plain.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        begin_plain!();
        plain.push(ch);
        i += ch.len_utf8();
    }

    flush!();
    let stable_prefix_len = unstable_from.unwrap_or(text.len());
    (out, restricted, stable_prefix_len)
}

fn visible_pieces(pieces: &[InlinePiece]) -> Vec<TextSpan> {
    pieces
        .iter()
        .filter_map(|piece| match piece {
            InlinePiece::Visible { text, style, .. } => {
                Some(TextSpan::styled(text.clone(), style.clone()))
            }
            InlinePiece::Hidden { .. } => None,
        })
        .collect()
}

fn projected_runs(pieces: &[InlinePiece]) -> Vec<AssistantProjectedRun> {
    let mut runs = Vec::new();
    let mut pending_hidden: Option<(Range<usize>, Option<usize>)> = None;
    for piece in pieces {
        match piece {
            InlinePiece::Hidden {
                source,
                restart_from,
            } => {
                pending_hidden = Some(match pending_hidden.take() {
                    Some((previous, previous_restart)) => (
                        previous.start..source.end,
                        previous_restart.or(*restart_from),
                    ),
                    None => (source.clone(), *restart_from),
                });
            }
            InlinePiece::Visible {
                text,
                style,
                source,
                restart_from: visible_restart,
            } => {
                let (owned_start, prefix_restart_from) = pending_hidden
                    .take()
                    .map_or((source.start, None), |(hidden, restart)| {
                        (hidden.start, restart)
                    });
                runs.push(AssistantProjectedRun {
                    display: text.clone(),
                    style: style.clone(),
                    owned: owned_start..source.end,
                    exact_visible: Some(source.clone()),
                    restart_from: *visible_restart,
                    prefix_restart_from,
                });
            }
        }
    }
    if let Some((hidden, restart_from)) = pending_hidden {
        if let Some(last) = runs.last_mut() {
            last.owned.end = hidden.end;
        } else {
            runs.push(AssistantProjectedRun {
                display: String::new(),
                style: StyleSpec::default(),
                owned: hidden,
                exact_visible: None,
                restart_from,
                prefix_restart_from: None,
            });
        }
    }
    runs
}

fn parse_inline_projected(
    text: &str,
    base: StyleSpec,
) -> (Vec<TextSpan>, Vec<AssistantProjectedRun>, bool, usize) {
    let (pieces, restricted, stable_len) = parse_inline_rec(text, base, 0, None);
    (
        visible_pieces(&pieces),
        projected_runs(&pieces),
        restricted,
        stable_len,
    )
}

// ---------------------------------------------------------------------------
// Finalized View adapter
// ---------------------------------------------------------------------------

pub(crate) const ASSISTANT_LIST_INDENT: usize = 2;

/// Structural horizontal inset for the assistant body.
pub(crate) const ASSISTANT_HORIZONTAL_INSET: u16 = 2;

/// Builds the finalized semantic assistant `View` from the shared document.
pub(crate) fn assistant_document_view(document: &AssistantDocument) -> View {
    View::vertical(|column| {
        column.children(document.rows.iter().map(assistant_row_view));
    })
    .fill_width()
    .padding(Insets::horizontal(ASSISTANT_HORIZONTAL_INSET))
}

pub(crate) fn assistant_row_view(row: &AssistantLogicalRow) -> View {
    let body = View::styled_text(row.spans.clone()).fill_width();
    match &row.layout {
        AssistantRowLayout::Plain
        | AssistantRowLayout::Heading
        | AssistantRowLayout::ListContinuation { .. } => body.into_view(),
        AssistantRowLayout::ListItem { depth, marker } => {
            list_item_row_view(*depth, *marker, body.into_view())
        }
    }
}

fn list_item_row_view(depth: usize, marker: AssistantMarker, body: View) -> View {
    let marker_text = match marker {
        AssistantMarker::Bullet => "• ".to_string(),
        AssistantMarker::Ordered { index } => format!("{index}. "),
    };
    let indent = depth.saturating_mul(ASSISTANT_LIST_INDENT);
    let body_column = indent.saturating_add(UnicodeWidthStr::width(marker_text.as_str()));
    let prefix = View::styled_text(vec![TextSpan::styled(
        format!("{}{}", " ".repeat(indent), marker_text),
        list_style(),
    )])
    .no_wrap()
    .into_view();
    let continuation = View::text(" ".repeat(body_column)).no_wrap().into_view();

    View::hanging(prefix, continuation, body).fill_width()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn concat_pieces(line: &RawLine) -> String {
    let mut s = String::new();
    for (_, text, _) in &line.pieces {
        s.push_str(text);
    }
    s
}

fn piece_source_len(line: &RawLine) -> usize {
    line.pieces.iter().map(|(_, t, _)| t.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::layout::compile_view;
    use crate::stream::StreamingSource;
    use crate::transcript::model::{AssistantSegment, SegmentKind};

    fn text_segs(text: &str) -> Vec<AssistantSegment> {
        vec![AssistantSegment::Text(text.to_string())]
    }

    fn compiled_text(view: &View, width: u16) -> Vec<String> {
        compile_view(view, width)
            .rows
            .iter()
            .map(|row| row.plain_text())
            .collect()
    }

    #[test]
    fn parser_preserves_markdown_semantics() {
        let document = parse_assistant(&text_segs(
            "# heading\n- item\n  - nested\n9. nine\n10. ten",
        ));
        assert!(matches!(
            document.rows[0].layout,
            AssistantRowLayout::Heading
        ));
        assert!(matches!(
            document.rows[1].layout,
            AssistantRowLayout::ListItem {
                depth: 0,
                marker: AssistantMarker::Bullet
            }
        ));
        assert!(matches!(
            document.rows[2].layout,
            AssistantRowLayout::ListItem {
                depth: 1,
                marker: AssistantMarker::Bullet
            }
        ));
        assert!(matches!(
            document.rows[3].layout,
            AssistantRowLayout::ListItem {
                marker: AssistantMarker::Ordered { index: 9 },
                ..
            }
        ));
        assert!(matches!(
            document.rows[4].layout,
            AssistantRowLayout::ListItem {
                marker: AssistantMarker::Ordered { index: 10 },
                ..
            }
        ));
    }

    #[test]
    fn finalized_view_compiles_lists_and_styles_without_row_adapter() {
        let view = assistant_document_view(&parse_assistant(&text_segs("**bold**\n- item")));
        let rows = compiled_text(&view, 40);
        assert!(rows.iter().any(|row| row.contains("bold")));
        assert!(rows.iter().any(|row| row.contains("• item")));
    }

    #[test]
    fn narrow_list_layout_remains_constructible_when_markers_consume_width() {
        let view =
            assistant_document_view(&parse_assistant(&text_segs("9. nine\n10. ten\n  - nested")));
        let block = compile_view(&view, 7);
        assert!(!block.physically_complete);
        let rows = block
            .rows
            .iter()
            .map(|row| row.plain_text())
            .collect::<Vec<_>>();
        assert!(rows.iter().any(|row| row.contains("9.")));
        assert!(rows.iter().any(|row| row.contains("10.")));
        assert!(rows.iter().any(|row| row.contains("•")));
    }

    #[test]
    fn thinking_to_text_has_a_real_semantic_blank_row() {
        let segments = vec![
            AssistantSegment::Thinking("thinking".to_string()),
            AssistantSegment::Text("\n\nanswer".to_string()),
        ];
        let document = parse_assistant(&segments);
        let view = assistant_document_view(&document);
        let rows = compiled_text(&view, 40);
        assert_eq!(rows, ["  thinking", "", "  answer"]);
    }

    #[test]
    fn assistant_stream_and_final_view_share_full_physical_rows() {
        let mut stream = crate::transcript::AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "plain\n- list");
        stream.seal();
        let snapshot = stream.snapshot();
        let final_view = assistant_document_view(&parse_assistant(stream.segments()));
        let content_width = 40u16.saturating_sub(ASSISTANT_HORIZONTAL_INSET * 2);
        let stream_rows =
            crate::stream::compile_stream(&snapshot.view, content_width, snapshot.source_end)
                .rows
                .into_iter()
                .map(|row| row.physical.placed(40, ASSISTANT_HORIZONTAL_INSET))
                .collect::<Vec<_>>();
        let final_rows = compile_view(&final_view, 40).rows;
        assert_eq!(stream_rows, final_rows);
    }

    #[test]
    fn wrapped_list_stream_and_final_view_match_full_physical_rows() {
        let cases = [
            "10. one two three four five",
            "- one two three four five",
            "  - nested one two three four five",
            "9. one two three\n10. one two three",
        ];
        for source in cases {
            let mut stream = crate::transcript::AssistantStream::new();
            stream.push_delta(SegmentKind::Text, source);
            stream.seal();
            let snapshot = stream.snapshot();
            for width in [12u16, 16, 24] {
                let content_width = width.saturating_sub(ASSISTANT_HORIZONTAL_INSET * 2);
                let stream_rows = crate::stream::compile_stream(
                    &snapshot.view,
                    content_width,
                    snapshot.source_end,
                )
                .rows
                .into_iter()
                .map(|row| row.physical.placed(width, ASSISTANT_HORIZONTAL_INSET))
                .collect::<Vec<_>>();
                let final_rows = compile_view(
                    &assistant_document_view(&parse_assistant(&text_segs(source))),
                    width,
                )
                .rows;
                assert_eq!(stream_rows, final_rows, "width={width}, source={source:?}");
            }
        }
    }

    #[test]
    fn markdown_utf8_prefixes_are_panic_free() {
        let corpus = [
            "plain **bold** _italic_ `code`",
            "# heading\n9. nine\n10. ten\n  - nested",
            "thinking\n\nanswer",
            "empty\n\nlines",
            "CJK 漢字 and combining e\u{301}",
            "emoji 👩‍🔬 and family 👨‍👩‍👧‍👦",
            "unfinished **bold _italic `code",
        ];
        for source in corpus {
            let mut boundaries = source
                .char_indices()
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            boundaries.push(source.len());
            for end in boundaries {
                let prefix = &source[..end];
                let document = parse_assistant(&text_segs(prefix));
                let mut stream = crate::transcript::AssistantStream::new();
                stream.push_delta(SegmentKind::Text, prefix);
                assert!(stream.snapshot().validate().is_ok());
                let _ = compile_view(&assistant_document_view(&document), 7);
            }
        }
    }

    #[test]
    fn sealed_stream_matches_final_view_across_unicode_and_markdown_cases() {
        let cases = [
            vec![AssistantSegment::Text("plain text".into())],
            vec![AssistantSegment::Text("**bold** _italic_ `code`".into())],
            vec![AssistantSegment::Text(
                "# heading\n9. nine\n10. ten\n  - nested".into(),
            )],
            vec![
                AssistantSegment::Thinking("thinking".into()),
                AssistantSegment::Text("answer".into()),
            ],
            vec![AssistantSegment::Text("empty\n\nlines".into())],
            vec![AssistantSegment::Text("漢字 e\u{301} 👩‍🔬".into())],
            vec![AssistantSegment::Text("unfinished **bold _italic".into())],
        ];
        for segments in cases {
            let mut stream = crate::transcript::AssistantStream::new();
            for segment in &segments {
                stream.push_delta(
                    match segment {
                        AssistantSegment::Thinking(_) => SegmentKind::Thinking,
                        AssistantSegment::Text(_) => SegmentKind::Text,
                    },
                    segment.text(),
                );
            }
            stream.seal();
            let snapshot = stream.snapshot();
            // A prefix-too-small surface is intentionally incomplete and is
            // rejected by the future terminal-size policy. Compare stream and
            // finalized Views only at widths where the hanging prefix fits.
            let widths: &[u16] = if segments
                .iter()
                .any(|segment| segment.text().contains("  - nested"))
            {
                &[40]
            } else if segments
                .iter()
                .any(|segment| segment.text().contains("9. nine"))
            {
                &[12, 40]
            } else {
                &[7, 12, 40]
            };
            for &width in widths {
                let content_width = width.saturating_sub(ASSISTANT_HORIZONTAL_INSET * 2);
                let stream_rows = crate::stream::compile_stream(
                    &snapshot.view,
                    content_width,
                    snapshot.source_end,
                )
                .rows
                .into_iter()
                .map(|row| row.physical.placed(width, ASSISTANT_HORIZONTAL_INSET))
                .collect::<Vec<_>>();
                let final_rows = compile_view(
                    &assistant_document_view(&parse_assistant(stream.segments())),
                    width,
                )
                .rows;
                assert_eq!(
                    stream_rows, final_rows,
                    "width={width}, segments={segments:?}"
                );
            }
        }
    }
}
