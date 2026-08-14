//! Cursor-related semantic helpers.
//!
//! Terminal x is the sum of stored EGC widths for an already-wrapped row.
//! Width is computed once per grapheme in that row, then treated as geometry.
//! Remeasuring `text[line_start..caret]` would re-segment a mid-cluster caret
//! and disagree with paint.

use std::ops::Range;

use crate::physical::grapheme_cell_width;
use unicode_segmentation::UnicodeSegmentation;

/// One EGC in a wrapped row, with width stored at tokenization.
struct RowGrapheme {
    start: usize,
    end: usize,
    width: usize,
}

fn row_graphemes(text: &str, start: usize, end: usize) -> Vec<RowGrapheme> {
    text[start..end]
        .grapheme_indices(true)
        .map(|(rel, grapheme)| {
            let start = start + rel;
            RowGrapheme {
                start,
                end: start + grapheme.len(),
                width: grapheme_cell_width(grapheme),
            }
        })
        .collect()
}

pub(super) fn logical_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut rows = Vec::new();
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if character == '\n' {
            rows.push(start..index);
            start = index + 1;
        }
    }
    rows.push(start..text.len());
    rows
}

pub(super) fn wrapped_line_index_by_start(rows: &[Range<usize>], position: usize) -> Option<usize> {
    let index = rows.partition_point(|row| row.start <= position);
    (index > 0).then_some(index - 1)
}

/// Display column of `caret` on a wrapped row, using stored grapheme widths.
///
/// A caret inside an EGC snaps to that cluster's leading edge.
pub(super) fn display_col_at(
    text: &str,
    line_start: usize,
    line_end: usize,
    caret: usize,
) -> usize {
    let caret = caret.clamp(line_start, line_end);
    let mut column = 0usize;
    for grapheme in row_graphemes(text, line_start, line_end) {
        if caret <= grapheme.start {
            return column;
        }
        if caret < grapheme.end {
            return column;
        }
        column = column.saturating_add(grapheme.width);
    }
    column
}

pub(super) fn cursor_for_display_col(
    text: &str,
    line_start: usize,
    line_end: usize,
    target: usize,
) -> usize {
    let mut column = 0usize;
    for grapheme in row_graphemes(text, line_start, line_end) {
        if column.saturating_add(grapheme.width) > target {
            return grapheme.start;
        }
        column = column.saturating_add(grapheme.width);
    }
    line_end
}
