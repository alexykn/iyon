use std::borrow::Cow;
use std::ops::Range;

use textwrap::Options;

use crate::physical::PhysicalStyle;
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::presentation::{
    WidthRule, WrapMode,
    ir::{TextCursorAnchor, TextView},
};

/// An atomic extended-grapheme cluster with style and optional source range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StyledGrapheme<'a> {
    pub(crate) text: Cow<'a, str>,
    pub(crate) width: usize,
    pub(crate) style: PhysicalStyle,
    pub(crate) source: Option<Range<usize>>,
}

/// A physical line of wrapped graphemes along with its display width and fit indicator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WrappedLine<'a> {
    pub(crate) graphemes: Vec<StyledGrapheme<'a>>,
    pub(crate) width: usize,
    pub(crate) fits: bool,
}

impl<'a> WrappedLine<'a> {
    pub(crate) fn new(graphemes: Vec<StyledGrapheme<'a>>, target_width: usize) -> Self {
        let width = graphemes.iter().map(|g| g.width).sum();
        let fits = if graphemes.is_empty() {
            true
        } else if target_width == 0 {
            false
        } else {
            width <= target_width
        };
        Self {
            graphemes,
            width,
            fits,
        }
    }
}

/// Internal span fragment within a hard line.
#[derive(Debug, Clone)]
struct SpanFragment<'a> {
    slice: &'a str,
    style: PhysicalStyle,
    source_start: Option<usize>,
}

/// Splits styled spans into hard logical lines (at `\n`), tokenizing each hard line
/// as a unified sequence of atomic [`StyledGrapheme`] clusters across span boundaries.
pub(crate) fn styled_hard_lines<'a, I>(spans: I) -> Vec<Vec<StyledGrapheme<'a>>>
where
    I: IntoIterator<Item = (&'a str, PhysicalStyle, Option<usize>)>,
{
    let mut hard_lines_fragments: Vec<Vec<SpanFragment<'a>>> = Vec::new();
    let mut current_fragments: Vec<SpanFragment<'a>> = Vec::new();

    for (span_text, style, source_base) in spans {
        let mut byte_offset = 0usize;
        for (piece_idx, piece) in span_text.split('\n').enumerate() {
            if piece_idx > 0 {
                hard_lines_fragments.push(std::mem::take(&mut current_fragments));
                byte_offset += 1; // '\n'
            }
            if !piece.is_empty() {
                let src_start = source_base.map(|base| base + byte_offset);
                current_fragments.push(SpanFragment {
                    slice: piece,
                    style,
                    source_start: src_start,
                });
            }
            byte_offset += piece.len();
        }
    }
    hard_lines_fragments.push(current_fragments);

    hard_lines_fragments
        .into_iter()
        .map(tokenize_hard_line)
        .collect()
}

fn tokenize_hard_line<'a>(fragments: Vec<SpanFragment<'a>>) -> Vec<StyledGrapheme<'a>> {
    if fragments.is_empty() {
        return Vec::new();
    }

    if fragments.len() == 1 {
        let frag = &fragments[0];
        let mut line = Vec::new();
        for (g_rel, g_text) in frag.slice.grapheme_indices(true) {
            let width = UnicodeWidthStr::width(g_text);
            let src = frag
                .source_start
                .map(|base| (base + g_rel)..(base + g_rel + g_text.len()));
            line.push(StyledGrapheme {
                text: Cow::Borrowed(g_text),
                width,
                style: frag.style,
                source: src,
            });
        }
        return line;
    }

    // Multiple fragments in this hard line. Concatenate and tokenize across boundaries.
    let mut full_text = String::new();
    let mut fragment_ranges: Vec<(Range<usize>, &SpanFragment<'a>)> =
        Vec::with_capacity(fragments.len());

    for frag in &fragments {
        let start = full_text.len();
        full_text.push_str(frag.slice);
        let end = full_text.len();
        fragment_ranges.push((start..end, frag));
    }

    let mut line = Vec::new();
    for (g_start, g_text) in full_text.grapheme_indices(true) {
        let g_end = g_start + g_text.len();
        let (start_range, start_frag) = fragment_ranges
            .iter()
            .find(|(r, _)| r.contains(&g_start))
            .unwrap();
        let (end_range, end_frag) = fragment_ranges
            .iter()
            .find(|(r, _)| r.contains(&(g_end - 1)))
            .unwrap();

        let style = start_frag.style;
        let source = match (start_frag.source_start, end_frag.source_start) {
            (Some(s_base), Some(e_base)) => {
                let s_offset = s_base + (g_start - start_range.start);
                let e_offset = e_base + (g_end - end_range.start);
                Some(s_offset..e_offset)
            }
            _ => None,
        };

        let text = if std::ptr::eq(*start_frag, *end_frag) {
            let rel_start = g_start - start_range.start;
            let rel_end = g_end - start_range.start;
            Cow::Borrowed(&start_frag.slice[rel_start..rel_end])
        } else {
            Cow::Owned(g_text.to_string())
        };

        let width = UnicodeWidthStr::width(g_text);
        line.push(StyledGrapheme {
            text,
            width,
            style,
            source,
        });
    }

    line
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StyledTextFlow<'a> {
    pub(crate) width: u16,
    pub(crate) rows: Vec<WrappedLine<'a>>,
    pub(crate) cursor: Option<(usize, usize)>,
}

pub(crate) fn text_flow<'a>(
    text: &TextView,
    hard_lines: Vec<Vec<StyledGrapheme<'a>>>,
    source: Option<&str>,
    max_width: u16,
    inherited_width: WidthRule,
) -> StyledTextFlow<'a> {
    let intrinsic_width = hard_lines
        .iter()
        .map(|line| line.iter().map(|grapheme| grapheme.width).sum::<usize>())
        .max()
        .unwrap_or(0);
    let cursor_needs_cell = text.cursor.is_some_and(|anchor| {
        !hard_lines.iter().flatten().any(|grapheme| {
            grapheme
                .source
                .as_ref()
                .is_some_and(|range| range.start == anchor.byte_offset)
        })
    });
    let intrinsic_width = intrinsic_width + usize::from(cursor_needs_cell);
    let width = match inherited_width {
        WidthRule::Fit => intrinsic_width.min(usize::from(max_width)) as u16,
        WidthRule::Fill => max_width,
    };
    let mut rows = if let Some(source) = source.filter(|_| text.cursor.is_some()) {
        wrap_input_styled_lines(&hard_lines, source, width)
    } else {
        wrap_styled_lines(&hard_lines, width, text.wrap)
    };
    let cursor = text.cursor.and_then(|anchor| {
        source.map(|source| {
            assert!(
                anchor.byte_offset <= source.len(),
                "text cursor anchor exceeds source length"
            );
            assert!(
                source.is_char_boundary(anchor.byte_offset),
                "text cursor anchor is not a UTF-8 boundary"
            );
            cursor_position(source, anchor, usize::from(width), &mut rows)
        })
    });
    StyledTextFlow {
        width,
        rows,
        cursor,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextFlowMetrics {
    pub(crate) width: u16,
    pub(crate) row_count: u16,
    pub(crate) fits: bool,
}

pub(crate) fn text_flow_metrics(text: &TextView, width: u16) -> TextFlowMetrics {
    let mut source_offset = 0usize;
    let hard_lines = styled_hard_lines(text.spans.iter().map(|span| {
        let base = Some(source_offset);
        source_offset += span.text.len();
        (span.text.as_str(), PhysicalStyle::default(), base)
    }));
    let source = text
        .spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>();
    let flow = text_flow(
        text,
        hard_lines,
        text.cursor.map(|_| source.as_str()),
        width,
        WidthRule::Fit,
    );
    TextFlowMetrics {
        width: flow.width,
        row_count: flow.rows.len().max(1) as u16,
        fits: flow.rows.iter().all(|row| row.fits),
    }
}

/// Generic grapheme-aware line-wrapping kernel.
///
/// Wraps pre-split hard lines of [`StyledGrapheme`]s to fit within `width` cells.
/// Extended grapheme clusters are never split internally.
pub(crate) fn wrap_styled_lines<'a>(
    hard_lines: &[Vec<StyledGrapheme<'a>>],
    width: u16,
    mode: WrapMode,
) -> Vec<WrappedLine<'a>> {
    let width = usize::from(width);
    let mut output = Vec::new();

    for line in hard_lines {
        if mode == WrapMode::NoWrap || width == 0 {
            output.push(WrappedLine::new(line.clone(), width));
            continue;
        }

        if line.is_empty() {
            output.push(WrappedLine::new(Vec::new(), width));
            continue;
        }

        if mode == WrapMode::Grapheme {
            for row in wrap_graphemes_exact(line, width) {
                output.push(WrappedLine::new(row, width));
            }
            continue;
        }

        // WrapMode::WordThenGrapheme
        for row in wrap_line_word_then_grapheme(line, width) {
            output.push(WrappedLine::new(row, width));
        }
    }

    output
}

/// Fallback hard breaking between extended grapheme clusters.
fn wrap_graphemes_exact<'a>(
    line: &[StyledGrapheme<'a>],
    width: usize,
) -> Vec<Vec<StyledGrapheme<'a>>> {
    let mut output = Vec::new();
    let mut current = Vec::new();
    let mut used = 0usize;

    for grapheme in line {
        if used > 0 && used.saturating_add(grapheme.width) > width {
            output.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(grapheme.clone());
        used = used.saturating_add(grapheme.width);
    }

    if !current.is_empty() {
        output.push(current);
    }

    output
}

/// Word-wrapping with UAX #14 break opportunities, falling back to grapheme-level
/// hard breaks when words exceed the available width.
fn wrap_line_word_then_grapheme<'a>(
    line: &[StyledGrapheme<'a>],
    width: usize,
) -> Vec<Vec<StyledGrapheme<'a>>> {
    if line.is_empty() {
        return vec![Vec::new()];
    }

    let mut full_text = String::new();
    let mut grapheme_byte_ends: Vec<usize> = Vec::with_capacity(line.len());

    for g in line {
        full_text.push_str(g.text.as_ref());
        grapheme_byte_ends.push(full_text.len());
    }

    let mut can_break_after = vec![false; line.len()];
    for (byte_offset, opportunity) in linebreaks(&full_text) {
        if opportunity == BreakOpportunity::Allowed || opportunity == BreakOpportunity::Mandatory {
            if let Ok(idx) = grapheme_byte_ends.binary_search(&byte_offset) {
                if idx < line.len() {
                    can_break_after[idx] = true;
                }
            }
        }
    }

    let mut output = Vec::new();
    let mut cursor = 0usize;

    while cursor < line.len() {
        let line_start = cursor;
        let mut used_width = 0usize;
        let mut last_legal_break: Option<usize> = None;

        while cursor < line.len() {
            let g = &line[cursor];

            if used_width + g.width <= width {
                used_width += g.width;
                if can_break_after[cursor] {
                    last_legal_break = Some(cursor + 1);
                }
                cursor += 1;
            } else {
                break;
            }
        }

        if cursor == line.len() {
            output.push(line[line_start..cursor].to_vec());
            break;
        }

        if let Some(break_at) = last_legal_break
            && break_at > line_start
        {
            output.push(line[line_start..break_at].to_vec());
            cursor = break_at;
        } else if cursor > line_start {
            output.push(line[line_start..cursor].to_vec());
        } else {
            // A single grapheme is wider than the available width (e.g. width=1, CJK/emoji width=2).
            // Do not split the grapheme cluster. Emit it on its own row.
            output.push(vec![line[cursor].clone()]);
            cursor += 1;
        }
    }

    output
}

/// Computes the proven composer input ranges without depending on the
/// application input module. The one reserved caret column and textwrap
/// behavior intentionally match the legacy input contract.
pub(crate) fn input_wrap_ranges(text: &str, width: u16) -> Vec<Range<usize>> {
    let wrap_width = usize::from(width.saturating_sub(1).max(1));
    let opts = Options::new(wrap_width).break_words(true);
    let mut visual = Vec::new();

    for logical in input_logical_line_ranges(text) {
        let slice = &text[logical.clone()];
        if slice.is_empty() {
            visual.push(logical.start..logical.start);
            continue;
        }

        let logical_visual_start = visual.len();
        let mut consumed_abs = logical.start;
        for piece in textwrap::wrap(slice, &opts) {
            let mut range = match piece {
                std::borrow::Cow::Borrowed(piece) => {
                    // Safety: textwrap's borrowed piece is a subslice of text.
                    let start = unsafe { piece.as_ptr().offset_from(text.as_ptr()) as usize };
                    start..start + piece.len()
                }
                std::borrow::Cow::Owned(piece) => {
                    input_map_owned_piece(text, consumed_abs, logical.end, &piece)
                }
            };

            // textwrap can return an empty piece for leading whitespace that
            // cannot share a row with the following word. Do not materialize
            // that piece: the next real word will absorb the skipped source
            // whitespace below.
            if range.is_empty() {
                continue;
            }

            if range.start > consumed_abs {
                let gap = &text[consumed_abs..range.start];
                if gap
                    .chars()
                    .all(|character| character == ' ' || character == '\t')
                {
                    if visual.len() > logical_visual_start {
                        let previous = visual
                            .last_mut()
                            .expect("logical visual rows should have a previous row");
                        // Keep the source anchor in the preceding row without
                        // creating a row whose only visible content is space.
                        previous.end = range.start;
                    } else {
                        // Preserve leading whitespace on the first real row.
                        range.start = consumed_abs;
                    }
                } else {
                    push_input_remainder(&mut visual, text, consumed_abs, range.start, wrap_width);
                }
            }

            // `textwrap` trims trailing whitespace from each piece. Keep all
            // of that whitespace attached to the piece instead of allowing an
            // overflow separator to become its own visual row.
            let mut end = range.end;
            for character in text[range.end..logical.end].chars() {
                if character != ' ' && character != '\t' {
                    break;
                }
                end += character.len_utf8();
            }
            visual.push(range.start..end);
            consumed_abs = end;
        }

        push_input_remainder(&mut visual, text, consumed_abs, logical.end, wrap_width);
    }

    if visual.is_empty() {
        visual.push(0..0);
    }
    visual
}

pub(crate) fn wrap_input_styled_lines<'a>(
    hard_lines: &[Vec<StyledGrapheme<'a>>],
    source: &str,
    width: u16,
) -> Vec<WrappedLine<'a>> {
    let ranges = input_wrap_ranges(source, width);
    let target_width = usize::from(width.saturating_sub(1).max(1));
    ranges
        .iter()
        .enumerate()
        .map(|(index, range)| {
            let graphemes = hard_lines
                .iter()
                .flatten()
                .filter(|grapheme| {
                    let Some(source_range) = grapheme.source.as_ref() else {
                        return false;
                    };
                    let overlaps = source_range.start < range.end && range.start < source_range.end;
                    overlaps
                        && ranges.iter().position(|candidate| {
                            candidate.start < source_range.end && source_range.start < candidate.end
                        }) == Some(index)
                })
                .cloned()
                .collect();
            WrappedLine::new(graphemes, target_width)
        })
        .collect()
}

fn cursor_position(
    source: &str,
    anchor: TextCursorAnchor,
    max_columns: usize,
    rows: &mut Vec<WrappedLine<'_>>,
) -> (usize, usize) {
    let max_columns = max_columns.max(1);
    let ranges = input_wrap_ranges(source, max_columns as u16);
    let mut row = ranges
        .partition_point(|range| range.start <= anchor.byte_offset)
        .saturating_sub(1);
    let line = &ranges[row];
    let mut column = unicode_width::UnicodeWidthStr::width(&source[line.start..anchor.byte_offset]);
    if column >= max_columns {
        row = row.saturating_add(1);
        column = 0;
    }

    while rows.len() <= row {
        rows.push(WrappedLine::new(
            Vec::new(),
            max_columns.saturating_sub(1).max(1),
        ));
    }
    if let Some(wrapped) = rows.get_mut(row) {
        wrapped.width = wrapped.width.max(column.saturating_add(1));
        wrapped.fits = wrapped.width <= max_columns;
    }
    (row, column)
}

fn push_input_remainder(
    visual: &mut Vec<Range<usize>>,
    text: &str,
    mut start: usize,
    end: usize,
    width: usize,
) {
    while start < end {
        let row_start = start;
        let mut row_end = start;
        let mut used = 0usize;
        for (offset, character) in text[start..end].char_indices() {
            let character_width = character.width().unwrap_or(0);
            if row_end > row_start && used.saturating_add(character_width) > width {
                break;
            }
            row_end = start + offset + character.len_utf8();
            used = used.saturating_add(character_width);
        }
        if row_end == row_start {
            let character = text[start..end]
                .chars()
                .next()
                .expect("input remainder should contain a character");
            row_end += character.len_utf8();
        }
        visual.push(row_start..row_end);
        start = row_end;
    }
}

fn input_map_owned_piece(
    text: &str,
    mut start: usize,
    max_end: usize,
    wrapped: &str,
) -> Range<usize> {
    while start < max_end && !wrapped.starts_with([' ', '\t']) {
        let Some(character) = text[start..].chars().next() else {
            break;
        };
        if character == ' ' || character == '\t' {
            start += character.len_utf8();
        } else {
            break;
        }
    }

    let mut end = start;
    let mut chars = wrapped.chars().peekable();
    while let Some(character) = chars.next() {
        if end < max_end {
            let source = text[end..]
                .chars()
                .next()
                .expect("input source character should exist");
            if character == source {
                end += source.len_utf8();
                continue;
            }
        }
        if character == '-' && chars.peek().is_none() {
            continue;
        }
    }
    start..end
}

fn input_logical_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if character == '\n' {
            ranges.push(start..index);
            start = index + 1;
        }
    }
    ranges.push(start..text.len());
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::PhysicalColor;
    fn fg(color: PhysicalColor) -> PhysicalStyle {
        PhysicalStyle {
            foreground: Some(color),
            ..PhysicalStyle::default()
        }
    }

    fn to_strings(rows: &[WrappedLine]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                row.graphemes
                    .iter()
                    .map(|g| g.text.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn input_wrap_attaches_overflow_space_to_the_adjacent_word() {
        assert_eq!(input_wrap_ranges("one two", 4), vec![0..4, 4..7]);
    }

    #[test]
    fn input_wrap_keeps_leading_whitespace_on_its_logical_line() {
        assert_eq!(input_wrap_ranges("one\n  two", 4), vec![0..3, 4..9]);
    }

    #[test]
    fn plain_words_wrap_at_spaces() {
        let hard = styled_hard_lines(vec![(
            "hello world whatever",
            PhysicalStyle::default(),
            None,
        )]);
        let wrapped = wrap_styled_lines(&hard, 12, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["hello world ", "whatever"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn long_word_hard_breaks_at_graphemes() {
        let hard = styled_hard_lines(vec![("abcdefghij", PhysicalStyle::default(), None)]);
        let wrapped = wrap_styled_lines(&hard, 4, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["abcd", "efgh", "ij"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn wide_emoji_never_split() {
        let hard = styled_hard_lines(vec![("😀😁😂😃", PhysicalStyle::default(), None)]);
        let wrapped = wrap_styled_lines(&hard, 3, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["😀", "😁", "😂", "😃"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn combining_characters_never_split() {
        let hard = styled_hard_lines(vec![(
            "e\u{301}e\u{301}e\u{301}",
            PhysicalStyle::default(),
            None,
        )]);
        let wrapped = wrap_styled_lines(&hard, 2, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["e\u{301}e\u{301}", "e\u{301}"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn preserve_hard_newlines() {
        let hard = styled_hard_lines(vec![(
            "line1\nline2\n\nline3",
            PhysicalStyle::default(),
            None,
        )]);
        let wrapped = wrap_styled_lines(&hard, 80, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["line1", "line2", "", "line3"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn combining_mark_across_styled_spans_merges_into_one_grapheme() {
        let style_a = fg(PhysicalColor::Indexed(1));
        let style_b = fg(PhysicalColor::Indexed(4));
        let hard = styled_hard_lines(vec![("e", style_a, Some(0)), ("\u{301}", style_b, Some(1))]);
        assert_eq!(hard.len(), 1);
        assert_eq!(hard[0].len(), 1, "must become ONE StyledGrapheme");
        let g = &hard[0][0];
        assert_eq!(g.text.as_ref(), "e\u{301}");
        assert_eq!(g.width, 1);
        assert_eq!(g.style, style_a, "style follows base codepoint");
        assert_eq!(g.source, Some(0..3), "source spans entire cluster");
    }

    #[test]
    fn zwj_sequence_across_spans_merges_into_one_grapheme() {
        let style_a = fg(PhysicalColor::Indexed(2));
        let style_b = fg(PhysicalColor::Indexed(3));
        let style_c = fg(PhysicalColor::Indexed(5));
        // Woman health worker: 👩 + ZWJ + ⚕ + variation selector 16
        let hard = styled_hard_lines(vec![
            ("👩", style_a, Some(10)),
            ("\u{200D}", style_b, Some(14)),
            ("⚕\u{FE0F}", style_c, Some(17)),
        ]);
        assert_eq!(hard.len(), 1);
        assert_eq!(
            hard[0].len(),
            1,
            "ZWJ sequence across spans is one atomic cluster"
        );
        let g = &hard[0][0];
        assert_eq!(g.text.as_ref(), "👩\u{200D}⚕\u{FE0F}");
        assert_eq!(g.width, 2);
        assert_eq!(g.style, style_a, "style follows base codepoint");
        assert_eq!(g.source, Some(10..23), "source spans full ZWJ sequence");
    }

    #[test]
    fn oversized_grapheme_marks_fits_false() {
        let hard = styled_hard_lines(vec![("漢字", PhysicalStyle::default(), Some(0))]);
        // Target width = 1 cell, but each CJK char is 2 cells wide.
        let wrapped = wrap_styled_lines(&hard, 1, WrapMode::WordThenGrapheme);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0].graphemes[0].text.as_ref(), "漢");
        assert_eq!(wrapped[0].width, 2);
        assert!(
            !wrapped[0].fits,
            "2-cell grapheme on 1-cell width does not fit"
        );
        assert_eq!(wrapped[1].graphemes[0].text.as_ref(), "字");
        assert_eq!(wrapped[1].width, 2);
        assert!(!wrapped[1].fits);
    }
}
