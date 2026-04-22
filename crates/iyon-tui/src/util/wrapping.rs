use std::ops::Range;

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
    if text.is_empty() {
        return std::iter::once(0..0).collect();
    }

    let wrap_width = usize::from(width.saturating_sub(1).max(1));
    let opts = Options::new(wrap_width).break_words(true);
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
