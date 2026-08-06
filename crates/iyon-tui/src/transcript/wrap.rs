use std::ops::Range;

use ratatui::text::{Line, Span};

use crate::input::wrap::{WrapConfig, wrap_ranges};
use crate::transcript::row::{LeftMargin, TranscriptRow};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TranscriptCommitBoundary {
    pub(crate) logical_row: usize,
    pub(crate) span_index: usize,
    pub(crate) byte_offset: usize,
}

impl TranscriptCommitBoundary {
    pub(crate) fn next_logical_row(logical_row: usize) -> Self {
        Self {
            logical_row: logical_row.saturating_add(1),
            span_index: 0,
            byte_offset: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WrappedTranscriptRows {
    pub(crate) rows: Vec<Line<'static>>,
    pub(crate) row_end_boundaries: Vec<TranscriptCommitBoundary>,
}

pub(crate) fn wrap_transcript_rows(
    width: u16,
    logical_rows: &[TranscriptRow],
    commit_boundary: TranscriptCommitBoundary,
) -> WrappedTranscriptRows {
    let mut rows = Vec::new();
    let mut row_end_boundaries = Vec::new();
    let start_row = commit_boundary.logical_row.min(logical_rows.len());

    for (logical_row, row) in logical_rows.iter().enumerate().skip(start_row) {
        let start = if logical_row == commit_boundary.logical_row {
            commit_boundary
        } else {
            TranscriptCommitBoundary {
                logical_row,
                span_index: 0,
                byte_offset: 0,
            }
        };
        wrap_transcript_logical_row(
            width.max(1),
            logical_row,
            row,
            start,
            &mut rows,
            &mut row_end_boundaries,
        );
    }

    WrappedTranscriptRows {
        rows,
        row_end_boundaries,
    }
}

fn wrap_transcript_logical_row(
    width: u16,
    logical_row: usize,
    row: &TranscriptRow,
    start: TranscriptCommitBoundary,
    rows: &mut Vec<Line<'static>>,
    row_end_boundaries: &mut Vec<TranscriptCommitBoundary>,
) {
    // Wrap within the content width (terminal width minus the left and right
    // margins) so the margins no longer steal columns and content keeps a
    // symmetric inset on both sides. Then re-apply a styled left margin to every
    // physical row so wrapped continuations keep the same hanging indent. For
    // bulleted header rows, the FIRST physical row carries the bullet prefix in
    // those margin columns so continuations align under the text after the
    // bullet rather than under the bullet dot.
    let inset = row.margin.left.saturating_add(row.margin.right);
    let content_width = width.saturating_sub(inset).max(1);
    let flattened = flatten_line_from_boundary(&row.line, start);
    let wrapped_ranges = wrap_ranges(&flattened.text, WrapConfig::transcript(content_width));
    let is_first_physical = start.span_index == 0 && start.byte_offset == 0;

    for (index, range) in wrapped_ranges.into_iter().enumerate() {
        let mut physical = line_from_flat_range(&row.line, &flattened.segments, range.clone());
        if !physical.spans.is_empty() {
            let prefix = if index == 0 && is_first_physical {
                row.first_prefix.as_str()
            } else {
                ""
            };
            physical = prepend_prefix(physical, row.margin, prefix);
        }
        let boundary = flat_offset_to_boundary(
            logical_row,
            &flattened.segments,
            flattened.text.len(),
            range.end,
        );
        push_row(rows, row_end_boundaries, physical, boundary);
    }
}

/// Prepends a styled left margin to the first span of a physical row. `first_prefix`
/// (e.g. a bullet) occupies the margin columns on the first line; an empty prefix
/// means plain margin spaces. When the content's first span shares the margin's
/// style (the common case), the prefix is merged into it so emission stays a single
/// styled span; otherwise it is inserted as its own leading span.
fn prepend_prefix(
    mut line: Line<'static>,
    margin: LeftMargin,
    first_prefix: &str,
) -> Line<'static> {
    if margin.left == 0 || line.spans.is_empty() {
        return line;
    }
    let prefix = if first_prefix.is_empty() {
        " ".repeat(margin.left as usize)
    } else {
        first_prefix.to_string()
    };
    if let Some(first) = line.spans.first_mut()
        && first.style == margin.style
    {
        let mut text = prefix;
        text.push_str(first.content.as_ref());
        first.content = text.into();
        return line;
    }
    line.spans.insert(0, Span::styled(prefix, margin.style));
    line
}

#[derive(Debug, Clone)]
struct FlatLine {
    text: String,
    segments: Vec<FlatSegment>,
}

#[derive(Debug, Clone, Copy)]
struct FlatSegment {
    flat_start: usize,
    flat_end: usize,
    span_index: usize,
    src_start: usize,
}

fn flatten_line_from_boundary(line: &Line<'static>, start: TranscriptCommitBoundary) -> FlatLine {
    let mut text = String::new();
    let mut segments = Vec::new();

    for (span_index, span) in line.spans.iter().enumerate().skip(start.span_index) {
        let mut byte_offset = if span_index == start.span_index {
            start.byte_offset.min(span.content.len())
        } else {
            0
        };

        while byte_offset < span.content.len() && !span.content.is_char_boundary(byte_offset) {
            byte_offset += 1;
        }

        let slice = &span.content[byte_offset..];
        if slice.is_empty() {
            continue;
        }

        let flat_start = text.len();
        text.push_str(slice);
        let flat_end = text.len();
        segments.push(FlatSegment {
            flat_start,
            flat_end,
            span_index,
            src_start: byte_offset,
        });
    }

    FlatLine { text, segments }
}

fn line_from_flat_range(
    template: &Line<'static>,
    segments: &[FlatSegment],
    range: Range<usize>,
) -> Line<'static> {
    let mut line = empty_like(template);

    for segment in segments {
        let overlap_start = segment.flat_start.max(range.start);
        let overlap_end = segment.flat_end.min(range.end);
        if overlap_start >= overlap_end {
            continue;
        }

        let source = &template.spans[segment.span_index];
        let src_start = segment.src_start + (overlap_start - segment.flat_start);
        let src_end = segment.src_start + (overlap_end - segment.flat_start);
        let piece = &source.content[src_start..src_end];
        push_span_text(&mut line, source, piece);
    }

    line
}

fn flat_offset_to_boundary(
    logical_row: usize,
    segments: &[FlatSegment],
    flat_len: usize,
    flat_offset: usize,
) -> TranscriptCommitBoundary {
    if flat_offset >= flat_len {
        return TranscriptCommitBoundary::next_logical_row(logical_row);
    }

    for segment in segments {
        if flat_offset < segment.flat_end {
            return TranscriptCommitBoundary {
                logical_row,
                span_index: segment.span_index,
                byte_offset: segment.src_start + (flat_offset - segment.flat_start),
            };
        }
    }

    if let Some(last) = segments.last() {
        return TranscriptCommitBoundary {
            logical_row,
            span_index: last.span_index,
            byte_offset: last.src_start + (flat_offset.saturating_sub(last.flat_start)),
        };
    }

    TranscriptCommitBoundary::next_logical_row(logical_row)
}

fn empty_like(line: &Line<'static>) -> Line<'static> {
    Line {
        style: line.style,
        alignment: line.alignment,
        spans: Vec::new(),
    }
}

fn push_row(
    rows: &mut Vec<Line<'static>>,
    boundaries: &mut Vec<TranscriptCommitBoundary>,
    row: Line<'static>,
    boundary: TranscriptCommitBoundary,
) {
    rows.push(row);
    boundaries.push(boundary);
}

fn push_span_text(line: &mut Line<'static>, source: &Span<'static>, text: &str) {
    if let Some(last) = line.spans.last_mut()
        && last.style == source.style
    {
        last.content.to_mut().push_str(text);
        return;
    }

    line.spans
        .push(Span::styled(text.to_string(), source.style));
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;

    use super::{TranscriptCommitBoundary, wrap_transcript_rows};
    use crate::transcript::row::{LeftMargin, TranscriptRow};

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn transcript_wrap_prefers_word_boundaries() {
        let rows = wrap_transcript_rows(
            10,
            &[TranscriptRow::new(Line::from("hello whatever amazing"))],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["hello ", "whatever ", "amazing"]);
    }

    #[test]
    fn transcript_wrap_hard_breaks_long_word() {
        let rows = wrap_transcript_rows(
            5,
            &[TranscriptRow::new(Line::from("abcdefgh"))],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["abcde", "fgh"]);
    }

    #[test]
    fn wrapped_continuation_lines_keep_the_hanging_indent() {
        // A 2-column margin wraps within width-2 and re-prefixes every physical
        // row, so both the indent is preserved AND content uses the full (reduced)
        // width instead of wrapping 2 columns early.
        let margin = LeftMargin::new(2, 0, Line::from("").style);
        let rows = wrap_transcript_rows(
            6,
            &[TranscriptRow::with_margin(Line::from("abcdefgh"), margin)],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["  abcd", "  efgh"]);
    }

    #[test]
    fn empty_content_row_stays_blank_despite_margin() {
        let margin = LeftMargin::new(2, 0, Line::from("").style);
        let rows = wrap_transcript_rows(
            6,
            &[TranscriptRow::with_margin(Line::from(""), margin)],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, [""],);
    }

    #[test]
    fn bulleted_row_alignment_follows_text_not_bullet_dot() {
        // A bulleted header row: the bullet + space occupy the 2 left-margin
        // columns on the first line, and continuation lines align under the text
        // (also 2 columns), not under the bullet dot.
        let style = Line::from("").style;
        // width 8 - (left 2 + right 2) = content width 4.
        let rows = wrap_transcript_rows(
            8,
            &[TranscriptRow::bullet("abcdefgh", style)],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["\u{25cf} abcd", "  efgh"]);
    }

    #[test]
    fn symmetric_right_margin_shrinks_content_width() {
        // With a right margin the content wraps earlier, leaving a blank right
        // gutter: left=2, right=2, width=6 => content width 2.
        let margin = LeftMargin::new(2, 2, Line::from("").style);
        let rows = wrap_transcript_rows(
            6,
            &[TranscriptRow::with_margin(Line::from("abcdefgh"), margin)],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["  ab", "  cd", "  ef", "  gh"]);
    }
}
