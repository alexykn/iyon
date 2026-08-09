//! Cursor-related semantic helpers.
//!
//! These helpers deliberately use byte ranges plus Unicode display widths. A
//! future layout bridge can provide soft-wrapped ranges without changing the
//! editor's movement contract.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

pub(super) fn cursor_for_display_col(
    text: &str,
    line_start: usize,
    line_end: usize,
    target: usize,
) -> usize {
    let mut width = 0;
    for (offset, grapheme) in text[line_start..line_end].grapheme_indices(true) {
        width += UnicodeWidthStr::width(grapheme);
        if width > target {
            return line_start + offset;
        }
    }
    line_end
}
