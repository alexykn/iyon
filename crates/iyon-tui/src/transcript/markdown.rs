//! Markdown rendering for assistant text.
//!
//! Turns the *text* portion of assistant segments into styled logical
//! [`TranscriptRow`]s: headings, bullet/ordered lists, and paragraphs with
//! inline emphasis (`**bold**`, `*italic*`, `` `code` ``). Thinking segments are
//! never treated as markdown — they pass through as muted + italic.
//!
//! Streaming is *tolerant*: an unclosed marker (`**unclosed`, `*ital`, `` `code ``)
//! renders literally rather than being suppressed. This is what makes streaming
//! safe without a fragile holdback stage between the backend and smoother — a
//! marker only becomes styled once its closer actually arrives, and the active
//! pane simply re-renders the already-shown text (no content hold / no new queue).
//!
//! # Freeze/spill safety
//! A row whose `line` spans do not 1:1 cover its source bytes — because it hides
//! markers (bold/italic/code) or because a structural marker hangs in the gutter
//! (list bullets, ordered numbers) — reports `restricted = true`. The active pane
//! freezes such rows whole-line, so a partial freeze never renders differently
//! live vs. committed, and the spill boundary always lands on a char-safe row
//! end. Unrestricted rows (plain paragraphs, thinking, headings) render 1:1 with
//! their source bytes, preserving the active pane's partial-line spill.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme;
use crate::transcript::model::{AssistantSegment, SegmentKind, thinking_style};
use crate::transcript::row::TranscriptRow;

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
pub(crate) fn render_assistant(segments: &[AssistantSegment]) -> RenderedAssistant {
    let raw_lines = flatten_lines(segments);
    let mut out = Vec::with_capacity(raw_lines.len());

    for line in raw_lines {
        out.push(render_line(&line));
    }

    RenderedAssistant { rows: out }
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
/// Renders a single logical line.
fn render_line(line: &RawLine) -> RenderedRow {
    let full = concat_pieces(line);
    let base = Style::default();

    if full.is_empty() {
        return RenderedRow {
            row: TranscriptRow::blank(),
            content_len: 0,
            restricted: false,
        };
    }

    if let Some(count) = header_run(&full) {
        return RenderedRow {
            row: render_header(line, count),
            content_len: piece_source_len(line),
            restricted: false,
        };
    }

    if let Some(depth) = ordered_depth(&full) {
        return render_ordered(line, depth);
    }

    if let Some(depth) = unordered_depth(&full) {
        return render_unordered(line, depth);
    }

    render_paragraph(line, base)
}

// ---------------------------------------------------------------------------
// Headings
// ---------------------------------------------------------------------------

/// `## Heading` — the `#`s stay visible and are accent-tinted (not enlarged).
fn render_header(line: &RawLine, _count: usize) -> TranscriptRow {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut seen_heading = false;

    for (kind, text, _) in &line.pieces {
        if *kind == SegmentKind::Thinking {
            spans.push(Span::styled(text.clone(), thinking_style()));
            continue;
        }
        if !seen_heading {
            spans.push(Span::styled(text.clone(), theme::markdown_header()));
            seen_heading = true;
        } else {
            spans.push(Span::styled(text.clone(), theme::markdown_header()));
        }
    }

    TranscriptRow::content(Line::from(spans), theme::markdown_header())
}

// ---------------------------------------------------------------------------
// Lists
// ---------------------------------------------------------------------------

fn render_unordered(line: &RawLine, depth: usize) -> RenderedRow {
    let trimmed = concat_pieces(line);
    let marker_len = unordered_marker_len(&trimmed).unwrap_or(0);
    let rest = trimmed[marker_len..].to_string();
    let style = theme::markdown_list();

    let spans = inline_spans(&rest, Style::default());
    let row = TranscriptRow::markdown_unordered(Line::from(spans), style, depth);
    RenderedRow {
        row,
        content_len: piece_source_len(line),
        restricted: true, // bullet replaces source marker
    }
}

fn render_ordered(line: &RawLine, depth: usize) -> RenderedRow {
    let full = concat_pieces(line);
    let sep_len = ordered_separator_len(&full).unwrap_or(0);
    let index = ordered_index(&full);
    let rest = full[sep_len..].to_string();
    let style = theme::markdown_list();

    let spans = inline_spans(&rest, Style::default());
    let row = TranscriptRow::markdown_ordered(Line::from(spans), style, index, depth);
    RenderedRow {
        row,
        content_len: piece_source_len(line),
        restricted: true, // marker hangs in gutter, not in line.spans -> whole-line freeze
    }
}

/// Marks up a plain (non-list) line into styled spans; groups consecutive
/// `**`/`*`/`` ` `` runs. Unclosed markers render literally (tolerant).
fn inline_spans(text: &str, base: Style) -> Vec<Span<'static>> {
    parse_inline(text, base)
        .into_iter()
        .map(|sp| Span::styled(sp.text, sp.style))
        .collect()
}

// ---------------------------------------------------------------------------
// Paragraphs
// ---------------------------------------------------------------------------

/// Plain paragraph with inline emphasis. Restricted iff any inline element hid
/// markers (bold/italic/code).
fn render_paragraph(line: &RawLine, base: Style) -> RenderedRow {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut restricted = false;

    for (kind, text, _) in &line.pieces {
        if *kind == SegmentKind::Thinking {
            spans.push(Span::styled(text.clone(), thinking_style()));
            continue;
        }
        let (parsed, is_restricted) = parse_inline_with_flag(text, base);
        restricted |= is_restricted;
        for sp in parsed {
            spans.push(Span::styled(sp.text, sp.style));
        }
    }

    RenderedRow {
        row: TranscriptRow::content(Line::from(spans), base),
        content_len: piece_source_len(line),
        restricted,
    }
}

// ---------------------------------------------------------------------------
// Detection helpers
// ---------------------------------------------------------------------------

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
/// spaces). One logical tab = [`LIST_INDENT`] columns. Returns depth and the
/// indentation-stripped text.
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

/// Returns the nesting depth if `full` is an unordered list item and its marker
/// length, else `None`.
fn unordered_depth(full: &str) -> Option<usize> {
    let (depth, trimmed) = list_depth(full);
    if unordered_marker_len(&trimmed).is_some() {
        Some(depth)
    } else {
        None
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

/// Returns the nesting depth if `full` is an ordered list item (`1. ` / `1) `).
fn ordered_depth(full: &str) -> Option<usize> {
    let (depth, trimmed) = list_depth(full);
    ordered_separator_len(&trimmed).is_some().then_some(depth)
}

/// Parses the numeric index of an ordered item.
fn ordered_index(trimmed: &str) -> usize {
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Inline emphasis
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineKind {
    Bold,
    Italic,
    Code,
}

/// A styled inline span (content + style).
#[derive(Debug)]
struct StyledSpan {
    text: String,
    style: Style,
}

fn inline_style(kind: InlineKind) -> Style {
    match kind {
        InlineKind::Bold => theme::markdown_bold(),
        InlineKind::Italic => theme::markdown_italic(),
        InlineKind::Code => theme::markdown_code(),
    }
}

fn inline_kind_at(rest: &str) -> Option<(InlineKind, usize)> {
    if rest.starts_with("**") {
        Some((InlineKind::Bold, 2))
    } else if let Some(c) = rest.chars().next() {
        if c == '*' || c == '_' {
            Some((InlineKind::Italic, c.len_utf8()))
        } else if c == '`' {
            Some((InlineKind::Code, 1))
        } else {
            None
        }
    } else {
        None
    }
}

fn find_closer(text: &str, from: usize, kind: InlineKind) -> Option<usize> {
    let rest = &text[from..];
    match kind {
        InlineKind::Bold => rest.find("**").map(|i| from + i),
        InlineKind::Italic => {
            let ch = rest.chars().next()?;
            rest.char_indices()
                .skip(1)
                .find(|(_, c)| *c == ch)
                .map(|(i, _)| from + i)
        }
        InlineKind::Code => rest.find('`').map(|i| from + i),
    }
}

fn parse_inline_with_flag(text: &str, base: Style) -> (Vec<StyledSpan>, bool) {
    let mut spans: Vec<StyledSpan> = Vec::new();
    let mut restricted = false;
    let mut pos = 0usize;
    let mut plain_start = 0usize;

    while pos < text.len() {
        let rest = &text[pos..];
        let marker = inline_kind_at(rest);
        match marker {
            Some((kind, marker_len)) => {
                let content_start = pos + marker_len;
                if let Some(closer) = find_closer(text, content_start, kind) {
                    if plain_start < pos {
                        spans.push(StyledSpan {
                            text: text[plain_start..pos].to_string(),
                            style: base,
                        });
                    }
                    let content = &text[content_start..closer];
                    spans.push(StyledSpan {
                        text: content.to_string(),
                        style: inline_style(kind),
                    });
                    restricted = true;
                    pos = closer + marker_len;
                    plain_start = pos;
                } else {
                    pos += marker_len;
                }
            }
            None => {
                let c = text[pos..].chars().next().expect("non-empty");
                pos += c.len_utf8();
            }
        }
    }

    if plain_start < text.len() {
        spans.push(StyledSpan {
            text: text[plain_start..].to_string(),
            style: base,
        });
    }

    (spans, restricted)
}

fn parse_inline(text: &str, base: Style) -> Vec<StyledSpan> {
    parse_inline_with_flag(text, base).0
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
        // Headings sit under the content margin (marker-free), not col 0.
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
        // bullet at depth 0: content column = 2 (outer) + 2 (marker) = 4.
        assert_eq!(out.rows[0].row.layout.content_column(), 4);
    }

    #[test]
    fn ordered_list_is_marker_local() {
        // Each ordered item's marker is sized to its own digits: `9.` is narrower
        // than `10.`. A later item never changes an earlier item's layout, which
        // is what keeps committed history append-only.
        let out = render_assistant(&text_segs("9. nine\n10. ten"));
        assert_eq!(out.rows.len(), 2);
        // Ordered markers hang in the gutter (not in line.spans), so the rows are
        // restricted, matching the tail-stability rule used by the new presenter.
        assert!(out.rows[0].restricted);
        assert_eq!(row_text(&out.rows[0].row), "nine");
        assert_eq!(row_text(&out.rows[1].row), "ten");
        let marker9 = out.rows[0].row.layout.marker.as_ref().unwrap();
        let marker10 = out.rows[1].row.layout.marker.as_ref().unwrap();
        assert_eq!(marker9.text(), "9. ");
        assert_eq!(marker10.text(), "10. ");
        // Marker-local: each item sizes to its own digits.
        assert_ne!(marker9.width(), marker10.width());
    }

    #[test]
    fn nested_unordered_list_sets_depth() {
        let out = render_assistant(&text_segs("- top\n  - nested"));
        assert_eq!(out.rows.len(), 2);
        assert_eq!(out.rows[0].row.layout.nesting_depth, 0);
        assert_eq!(out.rows[1].row.layout.nesting_depth, 1);
        // nested content column = 2 + marker(2) + 1*LIST_INDENT(2) = 6.
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
