//! Markdown rendering for assistant text.
//!
//! Turns `AssistantSegment`s into a **shared, width-independent semantic
//! document** (`AssistantDocument`), consumed by two adapters:
//!
//! ```text
//!                 AssistantDocument
//!                 /              \
//!                /                \
//!   finalized View adapter    streaming row adapter
//!        assistant_document_view     stream_rows
//! ```
//!
//! There is exactly **one** Markdown interpretation. Any fix to bold/italic/
//! code/heading/list classification/nesting/Thinking styling happens in
//! `parse_assistant` before the adapters split.
//!
//! The document holds no Ratatui, `TranscriptRow`, `Rect`, wrapping, or terminal
//! types. `AssistantLogicalRow` carries:
//!
//! * `spans` — semantic `TextSpan`s (appearance intent);
//! * `layout` — plain vs list-item (structure/geometry intent);
//! * `style` — the row-level gutter/prefix style;
//! * `source` — source stability / streaming bookkeeping (`content_len`,
//!   `has_newline`, `stable_prefix_len`), used only by the
//!   still-special active + pinned source-backed assistant path.
//!
//! Streaming is *tolerant*: an unclosed marker (`**unclosed`, `*ital`, `` `code ``)
//! renders literally rather than being suppressed. A marker only becomes styled
//! once its closer actually arrives, keeping streaming safe.
//!
//! The inline parser is **extended-grapheme safe and never panics on any valid
//! UTF-8 prefix** (a live stream feeds partial source). Every offset used to
//! slice `&str` is a byte offset derived from char/byte boundaries.

use std::ops::Range;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

use crate::presentation::{
    ColorSpec, Decoration, Insets, IntoView, RowChild, StyleSpec, TextAttributeSpec, TextSpan,
    ThemeKey, View, WidthRule,
};
use crate::theme;
use crate::transcript::model::{AssistantSegment, SegmentKind};
use crate::transcript::row::TranscriptRow;

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
    /// Row-level (gutter/prefix) style. Used by the streaming adapter to style
    /// hanging markers/margins; the finalized View ignores it (spans carry the
    /// visible styled content and the marker is a styled `RowChild`).
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

/// A `TranscriptRow` plus freeze metadata for the active (streaming) pane.
#[derive(Debug, Clone)]
pub(crate) struct RenderedRow {
    pub(crate) row: TranscriptRow,
    /// Source byte length of this logical row's line (excluding the trailing
    /// `\n`). Always the full source line length, even when markdown hides or
    /// replaces some of those bytes (e.g. `**bold**` hides the `**`).
    pub(crate) content_len: usize,
    /// True when this row hides/replaces source bytes (bold/italic/code, list
    /// bullets). The active pane must freeze such rows whole-line.
    pub(crate) restricted: bool,
}

/// Result of rendering assistant segments into logical rows.
#[derive(Debug, Clone)]
pub(crate) struct RenderedAssistant {
    pub(crate) rows: Vec<RenderedRow>,
}

/// Render the text portions of `segments` into styled [`TranscriptRow`]s.
///
/// INTERNAL ASSISTANT STREAM SEMANTICS. This is the streaming compatibility
/// adapter over the shared document; it exists only for the active source-backed
/// assistant engine.
pub(crate) fn render_assistant(segments: &[AssistantSegment]) -> RenderedAssistant {
    RenderedAssistant {
        rows: stream_rows(&parse_assistant(segments)),
    }
}

/// Streaming adapter: maps the shared document back into the exact existing
/// `RenderedRow`/`TranscriptRow` representation (marker/nesting geometry,
/// source metadata). Behavior-identical to the historical `render_assistant`.
pub(crate) fn stream_rows(document: &AssistantDocument) -> Vec<RenderedRow> {
    document.rows.iter().map(logical_row_to_rendered).collect()
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
// Streaming adapter: document -> RenderedRow / TranscriptRow
// ---------------------------------------------------------------------------

fn logical_row_to_rendered(row: &AssistantLogicalRow) -> RenderedRow {
    let line = Line::from(
        row.spans
            .iter()
            .map(|sp| Span::styled(sp.text.clone(), spec_to_style(&sp.style)))
            .collect::<Vec<_>>(),
    );

    let rendered = match &row.layout {
        AssistantRowLayout::Plain
        | AssistantRowLayout::Heading
        | AssistantRowLayout::ListContinuation { .. } => {
            if row.spans.is_empty() {
                TranscriptRow::blank()
            } else {
                TranscriptRow::content(line, spec_to_style(&row.style))
            }
        }
        AssistantRowLayout::ListItem {
            depth,
            marker: AssistantMarker::Bullet,
        } => TranscriptRow::markdown_unordered(line, spec_to_style(&row.style), *depth),
        AssistantRowLayout::ListItem {
            depth,
            marker: AssistantMarker::Ordered { index },
        } => TranscriptRow::markdown_ordered(line, spec_to_style(&row.style), *index, *depth),
    };

    RenderedRow {
        row: rendered,
        content_len: row.source.content_len,
        restricted: matches!(row.layout, AssistantRowLayout::ListItem { .. })
            || row.projected_runs.iter().any(|run| {
                run.exact_visible
                    .as_ref()
                    .is_some_and(|visible| *visible != run.owned)
            }),
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

/// INTERNAL ASSISTANT STREAM SEMANTICS. Compatibility bridge that lowers a
/// semantic `StyleSpec` to the Ratatui `Style` the legacy row IR needs. Mirrors
/// the theme-key mapping in the presentation resolver so both adapters agree.
fn spec_to_style(spec: &StyleSpec) -> Style {
    let mut style = Style::default();
    if let Some(foreground) = &spec.foreground {
        style.fg = Some(match foreground {
            ColorSpec::Theme(key) => resolve_theme_fg(&key.0),
            ColorSpec::Ansi(value) => Color::Indexed(*value),
            ColorSpec::Rgb { r, g, b } => Color::Rgb(*r, *g, *b),
        });
    }
    if spec.attributes.bold == Some(true) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if spec.attributes.italic == Some(true) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if spec.attributes.dim == Some(true) {
        style = style.add_modifier(Modifier::DIM);
    }
    if spec.attributes.underline == Some(true) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if spec.attributes.reversed == Some(true) {
        style = style.add_modifier(Modifier::REVERSED);
    }
    style
}

fn resolve_theme_fg(key: &str) -> Color {
    match key {
        "text.muted" | "surface.default" => theme::muted().fg.unwrap_or(Color::Reset),
        "markdown.header" => theme::markdown_header().fg.unwrap_or(Color::Reset),
        "markdown.bold" => theme::markdown_bold().fg.unwrap_or(Color::Reset),
        "markdown.italic" => theme::markdown_italic().fg.unwrap_or(Color::Reset),
        "markdown.code" => theme::markdown_code().fg.unwrap_or(Color::Reset),
        "markdown.list" => theme::markdown_list().fg.unwrap_or(Color::Reset),
        _ => Color::Reset,
    }
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
        depth: columns / crate::transcript::row::LIST_INDENT,
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

/// Structural horizontal inset for the assistant body (matches the historical
/// `RowLayout::content()` outer margin of 2).
pub(crate) const ASSISTANT_HORIZONTAL_INSET: u16 = 2;

/// Builds the finalized semantic assistant `View` from the shared document.
///
/// `first_unit` is a transitional visual-compatibility flag: the historical
/// first-assistant top blank is reproduced structurally when the assistant is the
/// first transcript unit. (Eventually the conversation surface owns first-unit
/// surface spacing.)
pub(crate) fn assistant_document_view(document: &AssistantDocument, first_unit: bool) -> View {
    let rows: Vec<View> = document.rows.iter().map(assistant_row_view).collect();
    let body = View::column(rows, 0).width(WidthRule::Fill);
    let body = if first_unit {
        // TRANSITIONAL PRESENTATION COMPATIBILITY:
        // first-unit top inset moves to ConversationSurface later.
        View::column(vec![View::spacer(1), body], 0)
    } else {
        body
    };

    View::box_(
        body,
        Decoration::default().padding(Insets {
            top: 0,
            right: ASSISTANT_HORIZONTAL_INSET,
            bottom: 0,
            left: ASSISTANT_HORIZONTAL_INSET,
        }),
    )
    .width(WidthRule::Fill)
}

pub(crate) fn assistant_row_view(row: &AssistantLogicalRow) -> View {
    let body = View::styled_text(row.spans.clone()).width(WidthRule::Fill);
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
    let marker_view = View::styled_text(vec![TextSpan::styled(marker_text, list_style())])
        .no_wrap()
        .into_view();

    let mut children: Vec<RowChild> = Vec::new();
    if depth > 0 {
        let indent = (depth as u16).saturating_mul(crate::transcript::row::LIST_INDENT as u16);
        children.push(RowChild::fixed(
            indent,
            View::text("").width(WidthRule::Fill).into_view(),
        ));
    }
    children.push(RowChild::content(marker_view));
    children.push(RowChild::flex(body));

    // gap = 0: the marker text already includes its trailing space, so the body
    // lands immediately after the marker at the exact historical content column.
    View::row(children, 0)
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
fn has_hidden_projection(row: &AssistantLogicalRow) -> bool {
    matches!(row.layout, AssistantRowLayout::ListItem { .. })
        || row.projected_runs.iter().any(|run| {
            run.exact_visible
                .as_ref()
                .is_some_and(|visible| *visible != run.owned)
        })
}

#[cfg(test)]
mod tests {
    //! Streaming-compatibility tests for the source-backed row adapter.
    use super::*;
    use crate::transcript::model::thinking_style;
    use ratatui::style::Modifier;

    fn text_segs(s: &str) -> Vec<AssistantSegment> {
        vec![AssistantSegment::Text(s.to_string())]
    }

    fn row_text(row: &TranscriptRow) -> String {
        row.line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn header_keeps_hashes_and_accents() {
        let out = render_assistant(&text_segs("## Hello\nworld"));
        assert_eq!(out.rows.len(), 2);
        assert!(!out.rows[0].restricted);
        assert_eq!(out.rows[0].content_len, "## Hello".len());
        let text = row_text(&out.rows[0].row);
        assert!(text.starts_with("## Hello"));
        for span in &out.rows[0].row.line.spans {
            assert_eq!(span.style.fg, theme::markdown_header().fg);
        }
        assert_eq!(out.rows[0].row.layout.content_column(), 2);
    }

    #[test]
    fn bold_hides_markers_and_marks_restricted() {
        let out = render_assistant(&text_segs("a **bold** c"));
        assert_eq!(out.rows.len(), 1);
        assert!(out.rows[0].restricted);
        assert_eq!(out.rows[0].content_len, "a **bold** c".len());
        assert_eq!(row_text(&out.rows[0].row), "a bold c");
        assert!(out.rows[0].row.line.spans.iter().any(|s| {
            s.content.as_ref() == "bold" && s.style.add_modifier.contains(Modifier::BOLD)
        }));
    }

    #[test]
    fn italic_hides_markers_and_marks_restricted() {
        // Regression: the historical renderer failed to close single-`*`/`_`
        // italics (it searched for the content's first char as the closer).
        let out = render_assistant(&text_segs("a *b* c"));
        assert_eq!(out.rows.len(), 1);
        assert!(out.rows[0].restricted);
        assert_eq!(row_text(&out.rows[0].row), "a b c");
        assert!(out.rows[0].row.line.spans.iter().any(|s| {
            s.content.as_ref() == "b" && s.style.add_modifier.contains(Modifier::ITALIC)
        }));
    }

    #[test]
    fn unclosed_bold_renders_literally() {
        let out = render_assistant(&text_segs("a **bold"));
        assert_eq!(out.rows.len(), 1);
        assert!(!out.rows[0].restricted);
        assert_eq!(row_text(&out.rows[0].row), "a **bold");
    }

    #[test]
    fn bullet_list_renders_marker_as_bullet() {
        let out = render_assistant(&text_segs("- item one\n- item two"));
        assert_eq!(out.rows.len(), 2);
        for r in &out.rows {
            assert!(r.restricted);
        }
        assert_eq!(row_text(&out.rows[0].row), "item one");
        assert_eq!(out.rows[0].content_len, "- item one".len());
        assert_eq!(out.rows[0].row.layout.content_column(), 4);
    }

    #[test]
    fn ordered_list_is_marker_local() {
        let out = render_assistant(&text_segs("9. nine\n10. ten"));
        assert_eq!(out.rows.len(), 2);
        assert!(out.rows[0].restricted);
        assert_eq!(row_text(&out.rows[0].row), "nine");
        assert_eq!(row_text(&out.rows[1].row), "ten");
        let marker9 = out.rows[0].row.layout.marker.as_ref().unwrap();
        let marker10 = out.rows[1].row.layout.marker.as_ref().unwrap();
        assert_eq!(marker9.text(), "9. ");
        assert_eq!(marker10.text(), "10. ");
        assert_ne!(marker9.width(), marker10.width());
    }

    #[test]
    fn nested_unordered_list_sets_depth() {
        let out = render_assistant(&text_segs("- top\n  - nested"));
        assert_eq!(out.rows.len(), 2);
        assert_eq!(out.rows[0].row.layout.nesting_depth, 0);
        assert_eq!(out.rows[1].row.layout.nesting_depth, 1);
        assert_eq!(out.rows[1].row.layout.content_column(), 6);
    }

    #[test]
    fn thinking_is_not_markdown() {
        let out = render_assistant(&[
            AssistantSegment::Thinking("## think\n".to_string()),
            AssistantSegment::Text("answer".to_string()),
        ]);
        assert_eq!(out.rows.len(), 2);
        assert!(!out.rows[0].restricted);
        assert_eq!(row_text(&out.rows[0].row), "## think");
        assert_eq!(out.rows[0].row.line.spans[0].style.fg, thinking_style().fg);
    }
}

#[cfg(test)]
mod correctness {
    //! Correctness tests for the shared semantic `AssistantDocument`, the single
    //! source of truth for markdown highlighting. These pin *correct* markdown
    //! semantics (not the historical renderer's frail edge behavior).
    use super::*;

    fn text_segs(s: &str) -> Vec<AssistantSegment> {
        vec![AssistantSegment::Text(s.to_string())]
    }

    fn tag(spec: &StyleSpec) -> String {
        let mut s = String::new();
        if spec.attributes.bold == Some(true) {
            s.push('B');
        }
        if spec.attributes.italic == Some(true) {
            s.push('I');
        }
        match &spec.foreground {
            Some(ColorSpec::Theme(k)) => match k.0.as_str() {
                "markdown.code" => s.push('C'),
                "markdown.header" => s.push('H'),
                "text.muted" => s.push('T'),
                _ => {}
            },
            _ => {}
        }
        if s.is_empty() {
            s.push('·');
        }
        s
    }

    fn row_tokens(doc: &AssistantDocument) -> Vec<Vec<(String, String)>> {
        doc.rows
            .iter()
            .map(|r| {
                r.spans
                    .iter()
                    .map(|sp| (sp.text.clone(), tag(&sp.style)))
                    .collect()
            })
            .collect()
    }

    fn single(s: &str) -> Vec<(String, String)> {
        let doc = parse_assistant(&text_segs(s));
        assert_eq!(doc.rows.len(), 1, "expected one row for {s:?}");
        row_tokens(&doc).remove(0)
    }

    #[test]
    fn plain_text_is_plain() {
        assert_eq!(
            single("hello world"),
            vec![("hello world".into(), "·".into())]
        );
    }

    #[test]
    fn bold() {
        assert_eq!(
            single("a **b** c"),
            vec![
                ("a ".into(), "·".into()),
                ("b".into(), "B".into()),
                (" c".into(), "·".into()),
            ]
        );
    }

    #[test]
    fn italic_star() {
        assert_eq!(
            single("a *b* c"),
            vec![
                ("a ".into(), "·".into()),
                ("b".into(), "I".into()),
                (" c".into(), "·".into()),
            ]
        );
    }

    #[test]
    fn italic_underscore() {
        assert_eq!(
            single("a _b_ c"),
            vec![
                ("a ".into(), "·".into()),
                ("b".into(), "I".into()),
                (" c".into(), "·".into()),
            ]
        );
    }

    #[test]
    fn code() {
        assert_eq!(
            single("a `b` c"),
            vec![
                ("a ".into(), "·".into()),
                ("b".into(), "C".into()),
                (" c".into(), "·".into()),
            ]
        );
    }

    #[test]
    fn bold_italic_combined_stars() {
        assert_eq!(single("***x***"), vec![("x".into(), "BI".into())]);
    }

    #[test]
    fn italic_inside_bold() {
        assert_eq!(
            single("**b _i_ b**"),
            vec![
                ("b ".into(), "B".into()),
                ("i".into(), "BI".into()),
                (" b".into(), "B".into()),
            ]
        );
    }

    #[test]
    fn bold_inside_italic() {
        assert_eq!(
            single("*b **i** b*"),
            vec![
                ("b ".into(), "I".into()),
                ("i".into(), "BI".into()),
                (" b".into(), "I".into()),
            ]
        );
    }

    #[test]
    fn multiplication_is_not_italic() {
        assert_eq!(single("2 * 3 * 4"), vec![("2 * 3 * 4".into(), "·".into())]);
    }

    #[test]
    fn unclosed_bold_renders_literally() {
        assert_eq!(single("a **b"), vec![("a **b".into(), "·".into())]);
    }

    #[test]
    fn unclosed_italic_renders_literally() {
        assert_eq!(single("a *b"), vec![("a *b".into(), "·".into())]);
    }

    #[test]
    fn unclosed_code_renders_literally() {
        assert_eq!(single("a `b"), vec![("a `b".into(), "·".into())]);
    }

    #[test]
    fn heading_styles_line() {
        let doc = parse_assistant(&text_segs("## hello"));
        assert_eq!(doc.rows.len(), 1);
        assert!(!has_hidden_projection(&doc.rows[0]));
        let text: String = doc.rows[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "## hello");
        assert!(doc.rows[0].spans.iter().all(|s| tag(&s.style) == "H"));
        assert!(matches!(doc.rows[0].layout, AssistantRowLayout::Heading));
    }

    #[test]
    fn hash_without_space_is_not_heading() {
        assert_eq!(single("#hello"), vec![("#hello".into(), "·".into())]);
    }

    #[test]
    fn thinking_is_muted_italic_not_markdown() {
        let doc = parse_assistant(&[AssistantSegment::Thinking("## think".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let text: String = doc.rows[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "## think", "thinking never parses as markdown");
        assert_eq!(tag(&doc.rows[0].spans[0].style), "IT");
        assert!(!has_hidden_projection(&doc.rows[0]));
    }
}

#[cfg(test)]
mod differential {
    //! Differential rendering: prove the finalized `View` adapter and the
    //! source-backed streaming adapter agree cell-for-cell on the SAME shared
    //! document. Because the two adapters legitimately render invisible whitespace
    //! (hanging indents, margins) with different fiducials, foreground only needs
    //! to match on non-whitespace symbols — the visible glyphs and glyph styling
    //! must be identical, which is what users actually see.
    use super::*;
    use ratatui::buffer::{Buffer, Cell};
    use ratatui::layout::Rect;

    use crate::presentation::layout::compile_view;
    use crate::terminal::ratatui::rows_to_lines;
    use crate::transcript::wrap::{TranscriptCommitBoundary, wrap_transcript_rows};

    fn text_segs(s: &str) -> Vec<AssistantSegment> {
        vec![AssistantSegment::Text(s.to_string())]
    }

    fn buffer_from_lines(width: u16, lines: &[Line<'static>]) -> Buffer {
        let area = Rect::new(0, 0, width, lines.len().max(1) as u16);
        let mut buffer = Buffer::empty(area);
        for (y, line) in lines.iter().enumerate() {
            let y = y as u16;
            buffer.set_style(Rect::new(0, y, width, 1), line.style);
            buffer.set_line(0, y, line, width);
        }
        buffer
    }

    fn render_pairs(doc: &AssistantDocument, width: u16) -> (Buffer, Buffer) {
        let rendered = stream_rows(doc);
        let rows: Vec<TranscriptRow> = rendered.iter().map(|r| r.row.clone()).collect();
        let wrapped = wrap_transcript_rows(width, &rows, TranscriptCommitBoundary::default());
        let oracle = buffer_from_lines(width, &wrapped.rows);

        let view = assistant_document_view(doc, false);
        let compiled = compile_view(&view, width);
        let candidate = buffer_from_lines(width, &rows_to_lines(&compiled.rows));

        (oracle, candidate)
    }

    fn is_whitespace(cell: &Cell) -> bool {
        cell.symbol().trim().is_empty()
    }

    fn assert_same(oracle: &Buffer, candidate: &Buffer, case: &str, width: u16) {
        assert_eq!(
            oracle.area.height, candidate.area.height,
            "{case} @ width {width}: row count mismatch (oracle {} vs candidate {})",
            oracle.area.height, candidate.area.height
        );
        for y in 0..oracle.area.height {
            for x in 0..oracle.area.width {
                let o = oracle.get(x, y);
                let c = candidate.get(x, y);
                assert_eq!(
                    o.symbol(),
                    c.symbol(),
                    "{case} @ width {width} cell({x},{y}) symbol"
                );
                if is_whitespace(o) {
                    assert_eq!(
                        o.bg, c.bg,
                        "{case} @ width {width} cell({x},{y}) background"
                    );
                    continue;
                }
                assert_eq!(o.fg, c.fg, "{case} @ width {width} cell({x},{y}) fg");
                assert_eq!(o.bg, c.bg, "{case} @ width {width} cell({x},{y}) bg");
                assert_eq!(
                    o.modifier, c.modifier,
                    "{case} @ width {width} cell({x},{y}) modifiers"
                );
            }
        }
    }

    fn differential(case: &str, source: &str) {
        for width in [80u16, 40, 20, 12, 8, 6, 3, 2, 1] {
            let doc = parse_assistant(&text_segs(source));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, case, width);
        }
    }

    #[test]
    fn plain_text_agrees() {
        differential("plain", "hello world, this is some plain assistant text.");
    }

    #[test]
    fn multiline_plain_agrees() {
        differential("multiline", "line one\nline two\nline three");
    }

    #[test]
    fn long_no_newline_paragraph_agrees() {
        differential(
            "long-paragraph",
            "This is a very long single paragraph with no newlines at all that should wrap repeatedly across many narrow physical rows and keep the hanging alignment consistent throughout the whole span of text.",
        );
    }

    #[test]
    fn thinking_agrees() {
        for width in [80u16, 40, 20, 12, 8, 6, 3, 2, 1] {
            let doc = parse_assistant(&[AssistantSegment::Thinking("reconsider".to_string())]);
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "thinking", width);
        }
    }

    #[test]
    fn thinking_then_text_agrees() {
        for width in [80u16, 40, 20, 12, 8, 6, 3, 2, 1] {
            let doc = parse_assistant(&[
                AssistantSegment::Thinking("a thought".to_string()),
                AssistantSegment::Text("answer text".to_string()),
            ]);
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "thinking-then-text", width);
        }
    }

    #[test]
    fn bold_agrees() {
        differential("bold", "a **bold** word and **another** one");
    }

    #[test]
    fn italic_agrees() {
        differential("italic", "a *italic* and _underscored_ word");
    }

    #[test]
    fn bold_italic_nested_agrees() {
        differential("bold-italic-nested", "**b _i_ b** and *b **i** b*");
    }

    #[test]
    fn inline_code_agrees() {
        differential("code", "run `cargo build` then `cargo run`");
    }

    #[test]
    fn unfinished_marker_agrees() {
        differential("unfinished", "an **unclosed marker and *single");
    }

    #[test]
    fn heading_agrees() {
        differential("heading", "## Section Heading\nbody after the heading");
    }

    #[test]
    fn bullet_list_agrees() {
        for width in [80u16, 40, 20, 12, 8] {
            let doc = parse_assistant(&text_segs("- first item\n- second item\n- third item"));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "bullets", width);
        }
    }

    #[test]
    fn ordered_list_agrees() {
        for width in [80u16, 40, 20, 12] {
            let doc = parse_assistant(&text_segs("8. eight\n9. nine\n10. ten\n11. eleven"));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "ordered", width);
        }
    }

    #[test]
    fn nested_bullet_agrees() {
        for width in [80u16, 40, 20, 12] {
            let doc = parse_assistant(&text_segs(
                "- top level\n  - nested one\n  - nested two\n- back to top",
            ));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "nested-bullet", width);
        }
    }

    #[test]
    fn nested_ordered_agrees() {
        for width in [80u16, 40, 20, 12] {
            let doc = parse_assistant(&text_segs("1. one\n2. two\n  - sub bullet\n  * another"));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "nested-ordered", width);
        }
    }

    #[test]
    fn list_wrapping_agrees() {
        for width in [80u16, 40, 20, 12, 8] {
            let doc = parse_assistant(&text_segs(
                "- this is a very long list item body that wraps across several physical rows at narrow widths and keeps continuation alignment",
            ));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "list-wrap", width);
        }
    }

    #[test]
    fn paragraph_to_list_agrees() {
        for width in [80u16, 40, 20, 12, 8] {
            let doc = parse_assistant(&text_segs("intro paragraph\n- item one\n- item two"));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "p-to-list", width);
        }
    }

    #[test]
    fn list_to_paragraph_agrees() {
        for width in [80u16, 40, 20, 12, 8] {
            let doc = parse_assistant(&text_segs("- item one\n- item two\nconcluding paragraph"));
            let (oracle, candidate) = render_pairs(&doc, width);
            assert_same(&oracle, &candidate, "list-to-p", width);
        }
    }

    #[test]
    fn empty_lines_agree() {
        differential("empty-lines", "one\n\ntwo\n\n\nthree");
    }

    #[test]
    fn wide_unicode_agrees() {
        differential(
            "wide-unicode",
            "emoji 😀 and CJK 漢字 and combining e\u{301} with **bold** — em dash",
        );
    }

    #[test]
    fn styled_across_wrap_boundary_agrees() {
        differential(
            "styled-wrap",
            "**bold text** that wraps across a boundary and *italic text* too",
        );
    }
}

#[cfg(test)]
mod no_panic {
    //! The streaming parser must never panic on any valid UTF-8 source, including
    //! every partial prefix a live stream can produce.
    use super::*;

    fn assert_prefix_safe(source: &str) {
        for (boundary, _) in source.char_indices() {
            parse_assistant(&[AssistantSegment::Text(source[..boundary].to_string())]);
        }
        parse_assistant(&[AssistantSegment::Text(source.to_string())]);
    }

    #[test]
    fn em_dash_prefixes_safe() {
        assert_prefix_safe("1. a long item \u{2014} with an em dash\n2. second \u{2014} line");
    }

    #[test]
    fn wide_prefixes_safe() {
        assert_prefix_safe(
            "text **bold \u{2014} \u{6F22}\u{5B57} \u{1F600}** tail and *italic* tail and `code \u{2014} \u{1F600}` then unclosed **emoji \u{1F600}",
        );
    }

    #[test]
    fn combining_and_zyg_prefixes_safe() {
        assert_prefix_safe(
            "e\u{301} + e\u{301} + family \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466} + flag \u{1F3F3}\u{FE0F}\u{200D}\u{1F308} + \u{1F44D}\u{1F3FD}",
        );
    }

    #[test]
    fn plain_cjk_prefixes_safe() {
        assert_prefix_safe(
            "\u{6F22}\u{5B57} \u{6D4B}\u{8BD5} \u{65E5}\u{672C}\u{8A9E} \u{3053}\u{3093}\u{306B}\u{3061}\u{306F} \u{2014} dash \u{2014} more",
        );
    }

    #[test]
    fn nested_marker_prefixes_safe() {
        assert_prefix_safe("**b _i_ b** and *b **i** b* and ***x***");
    }

    #[test]
    fn exhaustive_complex_prefixes_safe() {
        let cases = [
            "—",
            "é",
            "e\u{301}",
            "漢字",
            "😀",
            "👨👩👧👦",
            "🏳️🌈",
            "text **bold — 漢字 😀** tail",
            "text *italic 👨👩👧👦* tail",
            "`code — 😀`",
            "unclosed **emoji 😀",
            "## Section 1: Emoji 😀\n### Section 2: CJK 漢字\nBody with `code` and **bold**",
            "- bullet item 1\n  - nested 2\n    - nested 3",
            "1. ordered item — with em-dash\n  - sub-bullet *italic*\n2. second item `code`",
        ];
        for case in cases {
            assert_prefix_safe(case);
        }
    }
}

#[cfg(test)]
pub(crate) mod correctness_invariants {
    use super::*;
    use crate::presentation::layout::compile_view;
    use crate::presentation::{RowChild, View, WidthRule};
    use crate::transcript::wrap::{TranscriptCommitBoundary, wrap_transcript_rows};

    #[test]
    fn tiny_widths_never_overflow_allocated_row_geometry() {
        for width in [12u16, 8, 6, 3, 2, 1] {
            let row = View::row(
                vec![
                    RowChild::fixed(2, View::text("•").into_view()),
                    RowChild::flex(
                        View::text("body text wrapping here")
                            .width(WidthRule::Fill)
                            .into_view(),
                    ),
                    RowChild::content(View::text("status").into_view()),
                ],
                1,
            );
            let block = compile_view(&row, width);
            assert!(
                block.width <= width,
                "row surface width ({}) exceeded available width ({width})",
                block.width
            );
            for r in &block.rows {
                assert!(r.width() <= usize::from(width));
            }
        }
    }

    #[test]
    fn fill_never_exceeds_available_width_at_tiny_widths() {
        for width in [12u16, 8, 6, 3, 2, 1] {
            let view = View::styled_text(vec![TextSpan::plain(
                "A long paragraph of text that should wrap cleanly.",
            )])
            .width(WidthRule::Fill)
            .into_view();
            let block = compile_view(&view, width);
            assert!(
                block.width <= width,
                "Fill view width ({}) exceeded available width ({width})",
                block.width
            );
            for r in &block.rows {
                assert!(r.width() <= usize::from(width));
            }
        }
    }

    #[test]
    fn egcs_never_split_across_lines_at_all_widths() {
        let text = "e\u{301} 😀 漢字 👩‍⚕️ 👨‍👩‍👧‍👦 🏳️‍🌈";
        for width in [80u16, 40, 20, 12, 8, 6, 3, 2, 1] {
            let doc = parse_assistant(&[AssistantSegment::Text(text.to_string())]);
            let rows: Vec<TranscriptRow> =
                stream_rows(&doc).iter().map(|r| r.row.clone()).collect();
            let wrapped = wrap_transcript_rows(width, &rows, TranscriptCommitBoundary::default());
            for row in &wrapped.rows {
                for span in &row.spans {
                    assert!(span.content.is_char_boundary(0));
                    assert!(span.content.is_char_boundary(span.content.len()));
                }
            }
        }
    }

    #[test]
    fn wide_egc_impossible_fit_preserves_uncommitted_suffix() {
        let doc = parse_assistant(&[AssistantSegment::Text("漢字".to_string())]);
        let rows: Vec<TranscriptRow> = stream_rows(&doc).iter().map(|r| r.row.clone()).collect();
        let wrapped = wrap_transcript_rows(1, &rows, TranscriptCommitBoundary::default());
        assert_eq!(wrapped.rows.len(), 2);
        // Source boundaries accurately record where in source each row ends:
        assert_eq!(wrapped.row_end_boundaries[0].byte_offset, 3);
        assert_eq!(
            wrapped.row_end_boundaries[1],
            TranscriptCommitBoundary::next_logical_row(0)
        );
        // But commit eligibility correctly reports that 0 oversized rows may enter native history:
        assert_eq!(
            wrapped.transferable_prefix_rows, 0,
            "zero oversized physical rows are committable at width 1"
        );
    }

    #[test]
    fn source_ranges_round_trip_across_styled_span_boundaries() {
        use ratatui::style::Color;

        let style_a = crate::terminal::ratatui::physical_style(Style::default().fg(Color::Red));
        let style_b = crate::terminal::ratatui::physical_style(Style::default().fg(Color::Blue));
        let hard = crate::presentation::wrap::styled_hard_lines(vec![
            ("prefix e", style_a, Some(0)),
            ("\u{301} suffix", style_b, Some(8)),
        ]);
        assert_eq!(hard.len(), 1);
        let graphemes = &hard[0];
        let combined = &graphemes[7];
        assert_eq!(combined.text.as_ref(), "e\u{301}");
        assert_eq!(combined.source, Some(7..10));

        let style_c = crate::terminal::ratatui::physical_style(Style::default().fg(Color::Green));
        let style_d = crate::terminal::ratatui::physical_style(Style::default().fg(Color::Yellow));
        let hard_zwj = crate::presentation::wrap::styled_hard_lines(vec![
            ("family: 👩", style_c, Some(0)),
            ("\u{200D}⚕\u{FE0F} done", style_d, Some(12)),
        ]);
        assert_eq!(hard_zwj.len(), 1);
        let zwj_g = &hard_zwj[0][8];
        assert_eq!(zwj_g.text.as_ref(), "👩\u{200D}⚕\u{FE0F}");
        assert_eq!(zwj_g.source, Some(8..21));
    }

    // --- Explicit source stability metadata tests ---

    #[test]
    fn parser_closed_bold_end_touching_is_unstable() {
        let doc = parse_assistant(&[AssistantSegment::Text("abc **bold**".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(!row.source.has_newline);
        assert_eq!(row.source.content_len, "abc **bold**".len());
        // Closer touches EOF, so stable_prefix_len stops before opener
        assert_eq!(row.source.stable_prefix_len, "abc ".len());
    }

    #[test]
    fn parser_closed_bold_pinned_by_following_source() {
        let doc = parse_assistant(&[AssistantSegment::Text("abc **bold** x".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(!row.source.has_newline);
        assert_eq!(row.source.content_len, "abc **bold** x".len());
        // The semantic transformation is pinned, but the trailing `x` EGC is
        // still open for append purposes.
        assert_eq!(row.source.stable_prefix_len, "abc **bold** ".len());
    }

    #[test]
    fn parser_closed_bold_with_newline_is_stable() {
        let doc = parse_assistant(&[AssistantSegment::Text("abc **bold**\n".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(row.source.has_newline);
        assert_eq!(row.source.content_len, "abc **bold**".len());
        assert_eq!(row.source.stable_prefix_len, row.source.content_len);
    }

    #[test]
    fn parser_unclosed_bold_stops_stability_before_opener() {
        let doc = parse_assistant(&[AssistantSegment::Text("abc **bold".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(!has_hidden_projection(row));
        assert!(!row.source.has_newline);
        assert_eq!(row.source.content_len, "abc **bold".len());
        // Stops before the potentially active "**"
        assert_eq!(row.source.stable_prefix_len, "abc ".len());
        assert!(row.source.stable_prefix_len < row.source.content_len);
    }

    #[test]
    fn parser_list_with_ordinary_body_holds_back_final_egc() {
        let doc = parse_assistant(&[AssistantSegment::Text("- item".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(!row.source.has_newline);
        assert_eq!(row.source.content_len, "- item".len());
        // The list semantics are pinned, but the final `m` EGC is still open.
        assert_eq!(row.source.stable_prefix_len, "- ite".len());
    }

    #[test]
    fn parser_list_with_newline_is_stable_through_raw_content() {
        let doc = parse_assistant(&[AssistantSegment::Text("- item\n".to_string())]);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(row.source.has_newline);
        assert_eq!(row.source.content_len, "- item".len());
        assert_eq!(row.source.stable_prefix_len, row.source.content_len);
    }

    #[test]
    fn parser_list_with_unfinished_inline_markdown_stops_stability() {
        let doc = parse_assistant(&[AssistantSegment::Text("- item **bo".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(!row.source.has_newline);
        assert_eq!(row.source.content_len, "- item **bo".len());
        // Stable prefix stops before "**" in body: "- item " (7 bytes)
        assert_eq!(row.source.stable_prefix_len, "- item ".len());
        assert!(row.source.stable_prefix_len < row.source.content_len);
    }

    #[test]
    fn parser_completed_list_row_is_stable_through_newline() {
        let doc = parse_assistant(&[AssistantSegment::Text("- item **bo\n".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(row.source.has_newline);
        assert_eq!(row.source.content_len, "- item **bo".len());
        assert_eq!(row.source.stable_prefix_len, row.source.content_len);
    }

    #[test]
    fn parser_nested_completed_transformation_touches_eof_is_unstable() {
        let doc = parse_assistant(&[AssistantSegment::Text("**b _i_ b**".to_string())]);
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert!(!row.source.has_newline);
        assert_eq!(row.source.content_len, "**b _i_ b**".len());
        assert_eq!(row.source.stable_prefix_len, 0);
    }

    #[test]
    fn parser_ambiguous_line_classifications_return_zero_stability() {
        let cases = ["#", "##", "-", "+", "*", "1", "12", "1.", "12)"];
        for case in cases {
            let doc = parse_assistant(&[AssistantSegment::Text(case.to_string())]);
            assert_eq!(doc.rows.len(), 1, "failed for {case:?}");
            assert_eq!(
                doc.rows[0].source.stable_prefix_len, 0,
                "expected stable_prefix_len == 0 for ambiguous classification {case:?}"
            );
        }
    }

    #[test]
    fn parser_pinned_line_classifications_are_egc_safe() {
        let cases = [
            "# heading",
            "## ",
            "- item",
            "+ item",
            "1. item",
            "12) item",
        ];
        for case in cases {
            let doc = parse_assistant(&[AssistantSegment::Text(case.to_string())]);
            assert_eq!(doc.rows.len(), 1, "failed for {case:?}");
            assert!(
                doc.rows[0].source.stable_prefix_len < doc.rows[0].source.content_len,
                "open classification {case:?} must hold back its final EGC"
            );
        }
    }

    #[test]
    fn parser_whitespace_only_prefixes_are_classification_ambiguous() {
        for source in [" ", "  ", "\t", " \t", "\t "] {
            let doc = parse_assistant(&[AssistantSegment::Text(source.to_string())]);
            let row = &doc.rows[0];
            assert_eq!(row.source.stable_prefix_len, 0, "source={source:?}");
            assert_eq!(row.source.content_len, source.len(), "source={source:?}");
        }
    }

    #[test]
    fn parser_continuations_apply_open_egc_safety() {
        let heading = parse_assistant_tail(
            &[AssistantSegment::Text("continued heading".to_string())],
            crate::stream::StreamOffset::new(5),
            Some(AssistantContinuation::Heading),
        );
        assert_eq!(
            heading.rows[0].source.stable_prefix_len,
            "continued headin".len()
        );

        let paragraph = parse_assistant_tail(
            &[AssistantSegment::Text("- item".to_string())],
            crate::stream::StreamOffset::new(5),
            Some(AssistantContinuation::Paragraph),
        );
        assert_eq!(paragraph.rows[0].source.stable_prefix_len, "- ite".len());
    }

    #[test]
    fn parser_list_offsets_stay_in_raw_source_coordinates() {
        let unordered = unordered_parts("- \t\t**bo").expect("unordered list");
        assert_eq!(unordered.0, 0);
        assert_eq!(unordered.1, 2);
        assert_eq!(unordered.2, "\t\t**bo");

        let ordered = ordered_parts("\t1. \t**bo").expect("ordered list");
        assert_eq!(ordered.0, 2);
        assert_eq!(ordered.1, 1);
        assert_eq!(ordered.2, 4);
        assert_eq!(ordered.3, "\t**bo");

        let doc = parse_assistant(&[AssistantSegment::Text("- \t\t**bo".to_string())]);
        let row = &doc.rows[0];
        assert!(has_hidden_projection(row));
        assert_eq!(row.source.content_len, 8);
        assert_eq!(row.source.stable_prefix_len, 4);
        assert!(row.source.stable_prefix_len <= row.source.content_len);
    }

    #[test]
    fn nested_projection_retains_outer_restart_context() {
        let source = "prefix **outer _inner content which wraps_ outer** suffix";
        let doc = parse_assistant(&[AssistantSegment::Text(source.to_string())]);
        let row = &doc.rows[0];

        let inner = row
            .projected_runs
            .iter()
            .find(|run| run.display.contains("inner"))
            .expect("nested italic run");
        assert_eq!(inner.restart_from, Some(source.find("**").unwrap()));
        assert_eq!(inner.style.attributes.bold, Some(true));
        assert_eq!(inner.style.attributes.italic, Some(true));

        let suffix = row
            .projected_runs
            .iter()
            .find(|run| run.display.contains("suffix"))
            .expect("plain suffix run");
        assert_eq!(suffix.restart_from, None);
    }

    #[test]
    fn list_tab_replacements_are_local_projection_atoms() {
        let source = "- **before\tinside** followed by a long tail";
        let doc = parse_assistant(&[AssistantSegment::Text(source.to_string())]);
        let row = &doc.rows[0];
        let tab_source = source.find('\t').unwrap();
        let tab_run = row
            .projected_runs
            .iter()
            .find(|run| run.owned.start == tab_source && run.owned.end == tab_source + 1)
            .expect("tab replacement run");

        assert_eq!(tab_run.display, "    ");
        assert_eq!(tab_run.exact_visible, None);
        assert_eq!(tab_run.restart_from, Some(2));
        assert!(
            row.projected_runs
                .iter()
                .any(|run| run.display == "before" && run.style.attributes.bold == Some(true))
        );
        assert!(
            row.projected_runs
                .iter()
                .any(|run| { run.display == "inside" && run.style.attributes.bold == Some(true) })
        );
        assert!(
            row.projected_runs
                .iter()
                .any(|run| run.display.contains("followed"))
        );
    }

    #[test]
    fn parser_list_tabs_expand_only_for_display() {
        let doc = parse_assistant(&[AssistantSegment::Text("- a\tb".to_string())]);
        let row = &doc.rows[0];
        let visible: String = row.spans.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(visible, "a    b");
        assert_eq!(row.source.content_len, "- a\tb".len());
        assert!(row.source.stable_prefix_len <= row.source.content_len);

        let indented = parse_assistant(&[AssistantSegment::Text("\t- item".to_string())]);
        assert!(matches!(
            indented.rows[0].layout,
            AssistantRowLayout::ListItem { depth: 2, .. }
        ));
    }

    #[test]
    fn every_parser_row_stability_frontier_is_raw_bounded() {
        let corpus = [
            "plain",
            "# heading",
            "  ",
            "- item",
            "- \t\t**bo",
            "\t- a\tb",
            "1. ordered",
            "\t1. \t**bo",
            "abc **bold** x",
        ];
        for source in corpus {
            for with_newline in [false, true] {
                let source = if with_newline {
                    format!("{source}\n")
                } else {
                    source.to_string()
                };
                let doc = parse_assistant(&[AssistantSegment::Text(source)]);
                for row in doc.rows {
                    assert!(row.source.stable_prefix_len <= row.source.content_len);
                }
            }
        }
    }
}
