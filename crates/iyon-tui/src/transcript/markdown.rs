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
//! A row that *hides* or *replaces* source bytes (bold/italic/code markers, list
//! bullets) reports `restricted = true`. The active pane freezes such rows
//! whole-line, so a partial freeze can never render differently in the committed
//! transcript than in the live pane (e.g. a half-frozen `**bo` would otherwise
//! show literally in one place and styled in the other). Unrestricted rows render
//! 1:1 with their source bytes, preserving the active pane's partial-line spill.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme;
use crate::transcript::model::{AssistantSegment, SegmentKind, thinking_style};
use crate::transcript::row::{LeftMargin, TranscriptRow};

/// A `TranscriptRow` plus freeze metadata for the active (streaming) pane.
#[derive(Debug, Clone)]
pub(crate) struct RenderedRow {
    pub(crate) row: TranscriptRow,
    /// Source byte length of this logical row's line (excluding the trailing
    /// `\n`). Always the full source line length, even when markdown hides or
    /// replaces some of those bytes (e.g. `**bold**` hides the `**`).
    pub(crate) content_len: usize,
    /// True when this row hides/replaces source bytes (bold/italic/code, list
    /// bullets). The active pane must freeze such rows whole-line so a partial
    /// freeze never renders differently live vs. committed.
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

/// Renders a single logical line, returning its row and freeze-restriction flag.
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
            restricted: false, // `#`s stay visible -> 1:1 with source
        };
    }

    if let Some(marker_len) = unordered_marker(&full) {
        return render_list(line, marker_len);
    }

    if let Some(marker_len) = ordered_marker(&full) {
        return render_ordered(line, marker_len);
    }

    render_paragraph(line, base)
}

// --- headings ---

/// `## Heading` — the `#`s stay visible and are accent-tinted (not enlarged).
fn render_header(line: &RawLine, count: usize) -> TranscriptRow {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut seen_heading = false;

    for (kind, text, _) in &line.pieces {
        if *kind == SegmentKind::Thinking {
            spans.push(Span::styled(text.clone(), thinking_style()));
            continue;
        }
        if !seen_heading {
            // The leading `#` run and its trailing text are both accent-tinted but
            // kept fully visible (compact headings, not enlarged).
            let trimmed_len = text.trim_start().len();
            let idx = text.len() - trimmed_len;
            // Everything from the first `#` onward is part of the heading line.
            spans.push(Span::styled(text.clone(), theme::markdown_header()));
            let _ = (idx, count);
            seen_heading = true;
        } else {
            spans.push(Span::styled(text.clone(), theme::markdown_header()));
        }
    }

    TranscriptRow::new(Line::from(spans))
}

// --- lists ---

/// Bullet list (`- `, `* `, `+ `): hides the source marker, renders a `•` bullet
/// in the margin. Restricted because `- ` != `• `.
fn render_list(line: &RawLine, marker_len: usize) -> RenderedRow {
    let trimmed = concat_pieces(line);
    let rest = trimmed[marker_len..].to_string();
    let style = theme::markdown_list();

    let mut spans: Vec<Span<'static>> = Vec::new();
    for sp in parse_inline(&rest, Style::default()) {
        spans.push(Span::styled(sp.text, sp.style));
    }

    let row = TranscriptRow {
        line: Line::from(spans),
        margin: LeftMargin::new(LeftMargin::INDENT, 0, style),
        first_prefix: "\u{25cf} ".to_string(),
    };
    RenderedRow {
        row,
        content_len: piece_source_len(line),
        restricted: true, // bullet replaces source marker
    }
}

/// Ordered list (`1. `, `42. `): keeps the number visible in the margin; 1:1.
fn render_ordered(line: &RawLine, marker_len: usize) -> RenderedRow {
    let trimmed = concat_pieces(line);
    let (marker_text, rest) = trimmed.split_at(marker_len);
    let style = theme::markdown_list();

    let mut spans: Vec<Span<'static>> = Vec::new();
    for sp in parse_inline(rest, Style::default()) {
        spans.push(Span::styled(sp.text, sp.style));
    }

    // `marker_text` includes the trailing space (or is just the digits), so the
    // number renders before the item text.
    let marker = marker_text.to_string();
    let row = TranscriptRow {
        line: Line::from(spans),
        margin: LeftMargin::new(LeftMargin::INDENT, 0, style),
        first_prefix: marker,
    };
    RenderedRow {
        row,
        content_len: piece_source_len(line),
        restricted: false, // number text is preserved -> 1:1
    }
}

// --- paragraphs ---

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
        row: TranscriptRow::new(Line::from(spans)),
        content_len: piece_source_len(line),
        restricted,
    }
}

// --- header / list detection ---

fn header_run(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let count = trimmed.chars().take_while(|c| *c == '#').count();
    if count == 0 || count > 6 {
        return None;
    }
    let after = &trimmed[count..];
    if after.is_empty() || after.starts_with(' ') {
        Some(count)
    } else {
        None
    }
}

fn unordered_marker(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let first = trimmed.chars().next()?;
    if matches!(first, '-' | '*' | '+') {
        let mut chars = trimmed.chars();
        chars.next();
        if chars.next() == Some(' ') {
            return Some(first.len_utf8() + 1);
        }
    }
    None
}

fn ordered_marker(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
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
        Some(digit_end + sep_len) // e.g. "1." alone
    } else if after.starts_with(' ') {
        // Include the trailing space so the item content starts at the first word.
        Some(digit_end + sep_len + 1)
    } else {
        None
    }
}

// --- inline emphasis ---

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

/// Parse inline emphasis in a single line fragment. Returns rendered spans plus
/// whether any marker was hidden (a restricted row).
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
                    // Unclosed: treat as literal, advance past the opener.
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

/// Parse inline emphasis, discarding the restricted flag (for callers that only
/// need the spans).
fn parse_inline(text: &str, base: Style) -> Vec<StyledSpan> {
    parse_inline_with_flag(text, base).0
}

// --- helpers ---

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
        // every span of the heading uses the header accent fg
        for span in &out.rows[0].row.line.spans {
            assert_eq!(span.style.fg, theme::markdown_header().fg);
        }
    }

    #[test]
    fn bold_hides_markers_and_marks_restricted() {
        let out = render_assistant(&text_segs("a **bold** c"));
        assert_eq!(out.rows.len(), 1);
        assert!(out.rows[0].restricted);
        assert_eq!(out.rows[0].content_len, "a **bold** c".len());
        let text = row_text(&out.rows[0].row);
        assert_eq!(text, "a bold c");
        // the bold span is styled
        assert!(out.rows[0].row.line.spans.iter().any(|s| {
            s.content.as_ref() == "bold" && s.style.add_modifier.contains(Modifier::BOLD)
        }));
    }

    #[test]
    fn unclosed_bold_renders_literally() {
        let out = render_assistant(&text_segs("a **bold"));
        assert_eq!(out.rows.len(), 1);
        // restricted is false because nothing was hidden — the marker stays visible
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
        // content excludes the marker; content_len retains the full source length
        assert_eq!(row_text(&out.rows[0].row), "item one");
        assert_eq!(out.rows[0].content_len, "- item one".len());
    }

    #[test]
    fn ordered_list_keeps_number() {
        let out = render_assistant(&text_segs("1. first\n2. second"));
        assert_eq!(out.rows.len(), 2);
        assert!(!out.rows[0].restricted);
        assert_eq!(row_text(&out.rows[0].row), "first");
        assert_eq!(out.rows[0].row.first_prefix, "1. ");
    }

    #[test]
    fn thinking_is_not_markdown() {
        let out = render_assistant(&[
            AssistantSegment::Thinking("## think\n".to_string()),
            AssistantSegment::Text("answer".to_string()),
        ]);
        // thinking "## think" is a plain muted line (not a header) — markdown only
        // applies to Text segments.
        assert_eq!(out.rows.len(), 2);
        assert!(!out.rows[0].restricted);
        assert_eq!(row_text(&out.rows[0].row), "## think");
        assert_eq!(out.rows[0].row.line.spans[0].style.fg, thinking_style().fg);
    }
}
