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
//!   `restricted`), used only by the still-special active + pinned source-backed
//!   assistant path.
//!
//! Streaming is *tolerant*: an unclosed marker (`**unclosed`, `*ital`, `` `code ``)
//! renders literally rather than being suppressed. A marker only becomes styled
//! once its closer actually arrives, keeping streaming safe.
//!
//! The inline parser is **extended-grapheme safe and never panics on any valid
//! UTF-8 prefix** (a live stream feeds partial source). Every offset used to
//! slice `&str` is a byte offset derived from char/byte boundaries.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::presentation::{
    ColorSpec, Decoration, Insets, RowChild, StyleSpec, TextAttributes, TextSpan, ThemeKey, View,
    WidthRule,
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

/// Source stability / streaming bookkeeping. Neither contains terminal width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssistantSourceMeta {
    /// Source byte length of this logical row's line (excluding trailing `\n`).
    pub(crate) content_len: usize,
    /// True when the row hides/replaces source bytes and must freeze whole-line.
    pub(crate) restricted: bool,
}

/// Parse `segments` once into the shared width-independent document.
pub(crate) fn parse_assistant(segments: &[AssistantSegment]) -> AssistantDocument {
    let raw_lines = flatten_lines(segments);
    AssistantDocument {
        rows: raw_lines.into_iter().map(parse_line).collect(),
    }
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
        for (idx, piece) in segment.text().split('\n').enumerate() {
            if idx > 0 {
                lines.push(RawLine {
                    pieces: std::mem::take(&mut cur),
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
        lines.push(RawLine { pieces: cur });
    }
    lines
}

// ---------------------------------------------------------------------------
// Document parse: assign semantic spans, layout, style, and source metadata.
// ---------------------------------------------------------------------------

fn parse_line(line: RawLine) -> AssistantLogicalRow {
    let full = concat_pieces(&line);

    if full.is_empty() {
        return AssistantLogicalRow {
            spans: Vec::new(),
            layout: AssistantRowLayout::Plain,
            style: StyleSpec::default(),
            source: AssistantSourceMeta {
                content_len: 0,
                restricted: false,
            },
        };
    }

    if let Some(_count) = header_run(&full) {
        return AssistantLogicalRow {
            spans: header_spans(&line),
            layout: AssistantRowLayout::Plain,
            style: header_style(),
            source: AssistantSourceMeta {
                content_len: piece_source_len(&line),
                restricted: false,
            },
        };
    }

    if let Some(depth) = ordered_depth(&full) {
        let (_sep_len, index, rest) = ordered_parts(&full);
        return AssistantLogicalRow {
            spans: body_inline(&rest),
            layout: AssistantRowLayout::ListItem {
                depth,
                marker: AssistantMarker::Ordered { index },
            },
            style: list_style(),
            source: AssistantSourceMeta {
                content_len: piece_source_len(&line),
                restricted: true,
            },
        };
    }

    if let Some(depth) = unordered_depth(&full) {
        let rest = unordered_rest(&full);
        return AssistantLogicalRow {
            spans: body_inline(&rest),
            layout: AssistantRowLayout::ListItem {
                depth,
                marker: AssistantMarker::Bullet,
            },
            style: list_style(),
            source: AssistantSourceMeta {
                content_len: piece_source_len(&line),
                restricted: true,
            },
        };
    }

    // Plain paragraph (may include Thinking pieces).
    let (spans, restricted) = paragraph_spans(&line);
    AssistantLogicalRow {
        spans,
        layout: AssistantRowLayout::Plain,
        style: StyleSpec::default(),
        source: AssistantSourceMeta {
            content_len: piece_source_len(&line),
            restricted,
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
        AssistantRowLayout::Plain => {
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
        restricted: row.source.restricted,
    }
}

// ---------------------------------------------------------------------------
// Semantic styles
// ---------------------------------------------------------------------------

const fn attr(bold: bool, italic: bool) -> TextAttributes {
    TextAttributes {
        bold,
        italic,
        underline: false,
        dim: false,
        reversed: false,
    }
}

fn themed_foreground(key: &str, attributes: TextAttributes) -> StyleSpec {
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
    if spec.attributes.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if spec.attributes.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if spec.attributes.dim {
        style = style.add_modifier(Modifier::DIM);
    }
    if spec.attributes.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if spec.attributes.reversed {
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

/// List-item body spans parsed for inline emphasis (base = plain).
fn body_inline(rest: &str) -> Vec<TextSpan> {
    parse_inline_text(rest, StyleSpec::default())
}

/// Paragraph spans: Thinking pieces pass through as muted+italic; Text pieces
/// get inline emphasis. Returns whether any inline marker was hidden.
fn paragraph_spans(line: &RawLine) -> (Vec<TextSpan>, bool) {
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut restricted = false;
    for (kind, text, _) in &line.pieces {
        if *kind == SegmentKind::Thinking {
            spans.push(TextSpan::styled(text.clone(), thinking_style_spec()));
            continue;
        }
        let (parsed, is_restricted) = parse_inline_text_with_flag(text, StyleSpec::default());
        restricted |= is_restricted;
        spans.extend(parsed);
    }
    (spans, restricted)
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

/// Counts leading list indentation, normalizing tabs (a tab is treated as 4
/// spaces). One logical tab = [`LIST_INDENT`] columns.
fn list_depth(line: &str) -> (usize, String) {
    let expanded = line.replace('\t', "    ");
    let indent_cols = expanded.chars().take_while(|c| *c == ' ').count();
    let depth = indent_cols / crate::transcript::row::LIST_INDENT;
    (depth, expanded.trim_start().to_string())
}

/// The length of an unordered marker `- ` / `* ` / `+ ` (symbol + space).
fn unordered_marker_len(trimmed_of_full_line: &str) -> Option<usize> {
    let first = trimmed_of_full_line.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        let mut chars = trimmed_of_full_line.chars();
        chars.next();
        if chars.next() == Some(' ') {
            return Some(first.len_utf8() + 1);
        }
    }
    None
}

/// Returns the nesting depth if `full` is an unordered list item.
fn unordered_depth(full: &str) -> Option<usize> {
    let (depth, trimmed) = list_depth(full);
    if unordered_marker_len(&trimmed).is_some() {
        Some(depth)
    } else {
        None
    }
}

/// The body text of an unordered item (after the marker and its space).
fn unordered_rest(full: &str) -> String {
    let (_depth, trimmed) = list_depth(full);
    let marker_len = unordered_marker_len(&trimmed).unwrap_or(0);
    trimmed[marker_len..].to_string()
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

/// Returns the nesting depth if `full` is an ordered list item.
fn ordered_depth(full: &str) -> Option<usize> {
    let (depth, trimmed) = list_depth(full);
    ordered_separator_len(&trimmed).is_some().then_some(depth)
}

/// Ordered item parts: (separator_len, parsed index, body text).
fn ordered_parts(full: &str) -> (usize, usize, String) {
    let (_depth, trimmed) = list_depth(full);
    let sep_len = ordered_separator_len(&trimmed).unwrap_or(0);
    let index = ordered_index(&trimmed);
    let rest = trimmed[sep_len..].to_string();
    (sep_len, index, rest)
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
struct EmphSpan {
    text: String,
    style: StyleSpec,
}

/// Adds the emphasis strength's attribute flags onto `base` (foreground kept, so
/// nested emphasis inherits the outer color while stacking modifiers).
fn combine_style(base: &StyleSpec, kind: Emphasis) -> StyleSpec {
    let mut style = base.clone();
    match kind {
        Emphasis::Italic => style.attributes.italic = true,
        Emphasis::Bold => style.attributes.bold = true,
        Emphasis::BoldItalic => {
            style.attributes.bold = true;
            style.attributes.italic = true;
        }
    }
    style
}

/// True when a delimiter run at byte offset `i` is flanked by whitespace / string
/// edges on both sides. Such detached delimiters never open or close emphasis
/// (`2 * 3`).
fn is_detached(text: &str, i: usize, run: usize) -> bool {
    let before = text[..i].chars().next_back();
    let after = if i + run <= text.len() {
        text[i + run..].chars().next()
    } else {
        None
    };
    let left_ws = before.map_or(true, |c| c.is_whitespace());
    let right_ws = after.map_or(true, |c| c.is_whitespace());
    left_ws && right_ws
}

/// Finds the closing delimiter for an opener: the first run of exactly `need`
/// delimiters at or after byte `from`. Runs of a different length are nested
/// emphasis and are skipped (consumed into the inner content). Returns the
/// closer's start byte offset and its run length.
fn find_closer(text: &str, from: usize, ch: char, need: usize) -> Option<(usize, usize)> {
    debug_assert!(text.is_char_boundary(from));
    let mut j = from;
    while j < text.len() {
        let c = text[j..].chars().next()?;
        if c != ch {
            j += c.len_utf8();
            continue;
        }
        let run = text[j..].chars().take_while(|c0| *c0 == ch).count();
        if run == need {
            return Some((j, run));
        }
        // ch is a 1-byte ASCII delimiter, so `run` delimiters occupy `run` bytes.
        j += run;
    }
    None
}

fn parse_inline_rec(text: &str, base: StyleSpec) -> (Vec<EmphSpan>, bool) {
    let mut out: Vec<EmphSpan> = Vec::new();
    let mut plain = String::new();
    let mut restricted = false;
    let mut i = 0usize;

    macro_rules! flush {
        () => {
            if !plain.is_empty() {
                out.push(EmphSpan {
                    text: std::mem::take(&mut plain),
                    style: base.clone(),
                });
            }
        };
    }

    while i < text.len() {
        debug_assert!(text.is_char_boundary(i));
        let ch = text[i..].chars().next().expect("valid byte index");
        if ch == '`' {
            let rest = &text[i + 1..];
            if let Some(rel) = rest.find('`') {
                flush!();
                let mut code_style = base.clone();
                code_style.foreground = Some(ColorSpec::Theme(ThemeKey::from("markdown.code")));
                out.push(EmphSpan {
                    text: rest[..rel].to_string(),
                    style: code_style,
                });
                restricted = true;
                // backtick opener (1) + content (rel) + closer (1) = `rel + 2`.
                i = i + 1 + rel + 1;
            } else {
                plain.push(ch);
                i += 1;
            }
            continue;
        }

        if ch == '*' || ch == '_' {
            let run = text[i..].chars().take_while(|c0| *c0 == ch).count();
            if is_detached(text, i, run) {
                for _ in 0..run {
                    plain.push(ch);
                }
                i += run;
                continue;
            }
            let (kind, open_len) = if run >= 3 {
                (Emphasis::BoldItalic, 3)
            } else if run == 2 {
                (Emphasis::Bold, 2)
            } else {
                (Emphasis::Italic, 1)
            };
            if let Some((close_start, close_run)) =
                find_closer(text, i + open_len, ch, open_len)
            {
                flush!();
                let inner = &text[i + open_len..close_start];
                let inner_base = combine_style(&base, kind);
                let (inner_spans, inner_restricted) = parse_inline_rec(inner, inner_base);
                out.extend(inner_spans);
                restricted = true;
                i = close_start + close_run;
                let _ = inner_restricted;
            } else {
                plain.push(ch);
                i += 1;
            }
            continue;
        }

        plain.push(ch);
        i += ch.len_utf8();
    }

    flush!();
    (out, restricted)
}

/// Parses inline emphasis into semantic spans, reporting whether any marker was
/// hidden (restricted).
fn parse_inline_text_with_flag(text: &str, base: StyleSpec) -> (Vec<TextSpan>, bool) {
    let (spans, restricted) = parse_inline_rec(text, base);
    let out = spans
        .into_iter()
        .map(|sp| TextSpan::styled(sp.text, sp.style))
        .collect();
    (out, restricted)
}

fn parse_inline_text(text: &str, base: StyleSpec) -> Vec<TextSpan> {
    parse_inline_text_with_flag(text, base).0
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

fn assistant_row_view(row: &AssistantLogicalRow) -> View {
    let body = View::styled_text(row.spans.clone()).width(WidthRule::Fill);
    match &row.layout {
        AssistantRowLayout::Plain => body,
        AssistantRowLayout::ListItem { depth, marker } => {
            list_item_row_view(*depth, *marker, body)
        }
    }
}

fn list_item_row_view(depth: usize, marker: AssistantMarker, body: View) -> View {
    let marker_text = match marker {
        AssistantMarker::Bullet => "• ".to_string(),
        AssistantMarker::Ordered { index } => format!("{index}. "),
    };
    let marker_view = View::styled_text(vec![TextSpan::styled(marker_text, list_style())])
        .no_wrap();

    let mut children: Vec<RowChild> = Vec::new();
    if depth > 0 {
        let indent = (depth as u16).saturating_mul(crate::transcript::row::LIST_INDENT as u16);
        children.push(RowChild::fixed(
            indent,
            View::text("").width(WidthRule::Fill),
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
mod tests {
    //! Streaming-compatibility tests for the source-backed row adapter.
    use super::*;
    use ratatui::style::Modifier;
    use crate::transcript::model::thinking_style;

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
        if spec.attributes.bold {
            s.push('B');
        }
        if spec.attributes.italic {
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
            .map(|r| r.spans.iter().map(|sp| (sp.text.clone(), tag(&sp.style))).collect())
            .collect()
    }

    fn single(s: &str) -> Vec<(String, String)> {
        let doc = parse_assistant(&text_segs(s));
        assert_eq!(doc.rows.len(), 1, "expected one row for {s:?}");
        row_tokens(&doc).remove(0)
    }

    #[test]
    fn plain_text_is_plain() {
        assert_eq!(single("hello world"), vec![("hello world".into(), "·".into())]);
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
        assert!(!doc.rows[0].source.restricted);
        let text: String = doc.rows[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "## hello");
        assert!(doc.rows[0].spans.iter().all(|s| tag(&s.style) == "H"));
        assert!(matches!(doc.rows[0].layout, AssistantRowLayout::Plain));
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
        assert!(!doc.rows[0].source.restricted);
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

    use crate::presentation::internal::compile_view;
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
        let candidate = buffer_from_lines(width, &compile_view(&view, width).rows);

        (oracle, candidate)
    }

    fn is_whitespace(cell: &Cell) -> bool {
        cell.symbol().trim().is_empty()
    }

    fn assert_same(oracle: &Buffer, candidate: &Buffer, case: &str, width: u16) {
        assert_eq!(
            oracle.area.height,
            candidate.area.height,
            "{case} @ width {width}: row count mismatch (oracle {} vs candidate {})",
            oracle.area.height,
            candidate.area.height
        );
        for y in 0..oracle.area.height {
            for x in 0..oracle.area.width {
                let o = oracle.get(x, y);
                let c = candidate.get(x, y);
                assert_eq!(o.symbol(), c.symbol(), "{case} @ width {width} cell({x},{y}) symbol");
                if is_whitespace(o) {
                    assert_eq!(o.bg, c.bg, "{case} @ width {width} cell({x},{y}) background");
                    continue;
                }
                assert_eq!(o.fg, c.fg, "{case} @ width {width} cell({x},{y}) fg");
                assert_eq!(o.bg, c.bg, "{case} @ width {width} cell({x},{y}) bg");
                assert_eq!(o.modifier, c.modifier, "{case} @ width {width} cell({x},{y}) modifiers");
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
        differential("bullets", "- first item\n- second item\n- third item");
    }

    #[test]
    fn ordered_list_agrees() {
        differential("ordered", "8. eight\n9. nine\n10. ten\n11. eleven");
    }

    #[test]
    fn nested_bullet_agrees() {
        differential("nested-bullet", "- top level\n  - nested one\n  - nested two\n- back to top");
    }

    #[test]
    fn nested_ordered_agrees() {
        differential("nested-ordered", "1. one\n2. two\n  - sub bullet\n  * another");
    }

    #[test]
    fn list_wrapping_agrees() {
        differential(
            "list-wrap",
            "- this is a very long list item body that wraps across several physical rows at narrow widths and keeps continuation alignment",
        );
    }

    #[test]
    fn paragraph_to_list_agrees() {
        differential("p-to-list", "intro paragraph\n- item one\n- item two");
    }

    #[test]
    fn list_to_paragraph_agrees() {
        differential("list-to-p", "- item one\n- item two\nconcluding paragraph");
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
        assert_prefix_safe("e\u{301} + e\u{301} + family \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466} + flag \u{1F3F3}\u{FE0F}\u{200D}\u{1F308} + \u{1F44D}\u{1F3FD}");
    }

    #[test]
    fn plain_cjk_prefixes_safe() {
        assert_prefix_safe("\u{6F22}\u{5B57} \u{6D4B}\u{8BD5} \u{65E5}\u{672C}\u{8A9E} \u{3053}\u{3093}\u{306B}\u{3061}\u{306F} \u{2014} dash \u{2014} more");
    }

    #[test]
    fn nested_marker_prefixes_safe() {
        assert_prefix_safe("**b _i_ b** and *b **i** b* and ***x***");
    }
}
