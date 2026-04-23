use std::ops::Range;

use ratatui::text::{Line, Span};
use textwrap::Options;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Default)]
pub(crate) struct WrapCache {
    key: Option<(u64, u16)>,
    ranges: Vec<Range<usize>>,
}

impl WrapCache {
    pub(crate) fn input_ranges<'a>(
        &'a mut self,
        text_revision: u64,
        text: &str,
        width: u16,
    ) -> &'a [Range<usize>] {
        let width = width.max(1);
        let key = (text_revision, width);

        if self.key != Some(key) {
            self.ranges = compute_wrapped_ranges(text, width);
            self.key = Some(key);
        }

        self.ranges.as_slice()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WidthPolicy {
    ReserveCursorColumn,
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WrapConfig {
    pub(crate) width: u16,
    pub(crate) width_policy: WidthPolicy,
    pub(crate) break_words: bool,
}

impl WrapConfig {
    pub(crate) fn input(width: u16) -> Self {
        Self {
            width,
            width_policy: WidthPolicy::ReserveCursorColumn,
            break_words: true,
        }
    }

    pub(crate) fn transcript(width: u16) -> Self {
        Self {
            width,
            width_policy: WidthPolicy::Exact,
            break_words: true,
        }
    }
}

pub(crate) fn cursor_xy(
    text: &str,
    cursor: usize,
    lines: &[Range<usize>],
    max_cols: u16,
) -> (u16, u16) {
    let mut i = lines
        .partition_point(|r| r.start <= cursor)
        .saturating_sub(1);
    let line = &lines[i];
    let mut col = text[line.start..cursor].width() as u16;

    if col >= max_cols {
        i += 1;
        col = 0;
    }

    (col, i as u16)
}

pub(crate) fn wrapped_line_index_by_start(lines: &[Range<usize>], pos: usize) -> Option<usize> {
    let idx = lines.partition_point(|r| r.start <= pos);
    if idx == 0 { None } else { Some(idx - 1) }
}

pub(crate) fn cursor_for_display_col(
    text: &str,
    line_start: usize,
    line_end: usize,
    target_col: usize,
) -> usize {
    let mut width_so_far = 0usize;
    for (i, grapheme) in text[line_start..line_end].grapheme_indices(true) {
        width_so_far += grapheme.width();
        if width_so_far > target_col {
            return line_start + i;
        }
    }
    line_end
}

pub(crate) fn compute_wrapped_ranges(text: &str, width: u16) -> Vec<Range<usize>> {
    wrap_ranges(text, WrapConfig::input(width))
}

pub(crate) fn wrap_ranges(text: &str, cfg: WrapConfig) -> Vec<Range<usize>> {
    if text.is_empty() {
        return std::iter::once(0..0).collect();
    }

    let wrap_width = match cfg.width_policy {
        WidthPolicy::ReserveCursorColumn => usize::from(cfg.width.saturating_sub(1).max(1)),
        WidthPolicy::Exact => usize::from(cfg.width.max(1)),
    };
    let opts = Options::new(wrap_width).break_words(cfg.break_words);
    let mut visual = Vec::<Range<usize>>::new();

    for logical in logical_line_ranges(text) {
        let slice = &text[logical.clone()];

        if slice.is_empty() {
            visual.push(logical.start..logical.start);
            continue;
        }

        let mut consumed_abs = logical.start;

        for piece in textwrap::wrap(slice, &opts) {
            let range = match piece {
                std::borrow::Cow::Borrowed(piece) => {
                    // Safety: piece is borrowed from slice and slice is a subslice of text.
                    let abs_start = unsafe { piece.as_ptr().offset_from(text.as_ptr()) as usize };
                    let abs_end = abs_start + piece.len();
                    abs_start..abs_end
                }
                std::borrow::Cow::Owned(piece) => {
                    map_owned_piece_to_range(text, consumed_abs, logical.end, &piece)
                }
            };

            let trailing_whitespace_bytes: usize = text[range.end..logical.end]
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .map(|c| c.len_utf8())
                .sum();

            let final_end = range.end + trailing_whitespace_bytes;
            visual.push(range.start..final_end);
            consumed_abs = final_end;
        }
    }

    if visual.is_empty() {
        visual.push(0..0);
    }
    visual
}

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
    logical_rows: &[Line<'static>],
    commit_boundary: TranscriptCommitBoundary,
) -> WrappedTranscriptRows {
    let mut rows = Vec::new();
    let mut row_end_boundaries = Vec::new();
    let start_row = commit_boundary.logical_row.min(logical_rows.len());

    for (logical_row, line) in logical_rows.iter().enumerate().skip(start_row) {
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
            line,
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
    line: &Line<'static>,
    start: TranscriptCommitBoundary,
    rows: &mut Vec<Line<'static>>,
    row_end_boundaries: &mut Vec<TranscriptCommitBoundary>,
) {
    let flattened = flatten_line_from_boundary(line, start);
    let wrapped_ranges = wrap_ranges(&flattened.text, WrapConfig::transcript(width.max(1)));

    for range in wrapped_ranges {
        let row = line_from_flat_range(line, &flattened.segments, range.clone());
        let boundary = flat_offset_to_boundary(
            logical_row,
            &flattened.segments,
            flattened.text.len(),
            range.end,
        );
        push_row(rows, row_end_boundaries, row, boundary);
    }
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

fn map_owned_piece_to_range(
    text: &str,
    mut start: usize,
    max_end: usize,
    wrapped: &str,
) -> Range<usize> {
    while start < max_end && !wrapped.starts_with([' ', '\t']) {
        let Some(ch) = text[start..].chars().next() else {
            break;
        };
        if ch == ' ' || ch == '\t' {
            start += ch.len_utf8();
        } else {
            break;
        }
    }

    let mut end = start;
    let mut chars = wrapped.chars().peekable();

    while let Some(ch) = chars.next() {
        if end < max_end {
            let src_ch = text[end..]
                .chars()
                .next()
                .expect("source char should exist");

            if ch == src_ch {
                end += src_ch.len_utf8();
                continue;
            }
        }

        if ch == '-' && chars.peek().is_none() {
            continue;
        }
    }

    start..end
}

fn logical_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0usize;

    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            out.push(start..i);
            start = i + 1;
        }
    }
    out.push(start..text.len());
    out
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;
    use std::ops::Range;

    use super::{
        TranscriptCommitBoundary, WrapConfig, compute_wrapped_ranges, wrap_ranges,
        wrap_transcript_rows,
    };

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
            &[Line::from("hello whatever amazing")],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["hello ", "whatever ", "amazing"]);
    }

    #[test]
    fn transcript_wrap_hard_breaks_long_word() {
        let rows = wrap_transcript_rows(
            5,
            &[Line::from("abcdefgh")],
            TranscriptCommitBoundary::default(),
        );

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["abcde", "fgh"]);
    }

    #[test]
    fn input_policy_reserves_one_column() {
        assert_eq!(compute_wrapped_ranges("abcdef", 5), vec![0..4, 4..6]);
    }

    #[test]
    fn transcript_policy_uses_exact_width() {
        assert_eq!(
            wrap_ranges("abcdef", WrapConfig::transcript(5)),
            vec![Range { start: 0, end: 5 }, Range { start: 5, end: 6 }]
        );
    }
}
