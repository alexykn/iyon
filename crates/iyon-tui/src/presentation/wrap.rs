use std::borrow::Cow;
use std::ops::Range;

use ratatui::style::Style;
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::presentation::WrapMode;

/// An atomic extended-grapheme cluster with style and optional source range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StyledGrapheme<'a> {
    pub(crate) text: Cow<'a, str>,
    pub(crate) width: usize,
    pub(crate) style: Style,
    pub(crate) source: Option<Range<usize>>,
}

impl<'a> StyledGrapheme<'a> {
    pub(crate) fn new(text: impl Into<Cow<'a, str>>, style: Style) -> Self {
        let text = text.into();
        let width = UnicodeWidthStr::width(text.as_ref());
        Self {
            text,
            width,
            style,
            source: None,
        }
    }

    pub(crate) fn with_source(
        text: impl Into<Cow<'a, str>>,
        style: Style,
        source: Range<usize>,
    ) -> Self {
        let text = text.into();
        let width = UnicodeWidthStr::width(text.as_ref());
        Self {
            text,
            width,
            style,
            source: Some(source),
        }
    }
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
    style: Style,
    source_start: Option<usize>,
}

/// Splits styled spans into hard logical lines (at `\n`), tokenizing each hard line
/// as a unified sequence of atomic [`StyledGrapheme`] clusters across span boundaries.
pub(crate) fn styled_hard_lines<'a, I>(spans: I) -> Vec<Vec<StyledGrapheme<'a>>>
where
    I: IntoIterator<Item = (&'a str, Style, Option<usize>)>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

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
    fn plain_words_wrap_at_spaces() {
        let hard = styled_hard_lines(vec![("hello world whatever", Style::default(), None)]);
        let wrapped = wrap_styled_lines(&hard, 12, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["hello world ", "whatever"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn long_word_hard_breaks_at_graphemes() {
        let hard = styled_hard_lines(vec![("abcdefghij", Style::default(), None)]);
        let wrapped = wrap_styled_lines(&hard, 4, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["abcd", "efgh", "ij"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn wide_emoji_never_split() {
        let hard = styled_hard_lines(vec![("😀😁😂😃", Style::default(), None)]);
        let wrapped = wrap_styled_lines(&hard, 3, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["😀", "😁", "😂", "😃"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn combining_characters_never_split() {
        let hard = styled_hard_lines(vec![("e\u{301}e\u{301}e\u{301}", Style::default(), None)]);
        let wrapped = wrap_styled_lines(&hard, 2, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["e\u{301}e\u{301}", "e\u{301}"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn preserve_hard_newlines() {
        let hard = styled_hard_lines(vec![("line1\nline2\n\nline3", Style::default(), None)]);
        let wrapped = wrap_styled_lines(&hard, 80, WrapMode::WordThenGrapheme);
        assert_eq!(to_strings(&wrapped), vec!["line1", "line2", "", "line3"]);
        assert!(wrapped.iter().all(|r| r.fits));
    }

    #[test]
    fn combining_mark_across_styled_spans_merges_into_one_grapheme() {
        let style_a = Style::default().fg(Color::Red);
        let style_b = Style::default().fg(Color::Blue);
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
        let style_a = Style::default().fg(Color::Green);
        let style_b = Style::default().fg(Color::Yellow);
        let style_c = Style::default().fg(Color::Magenta);
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
        let hard = styled_hard_lines(vec![("漢字", Style::default(), Some(0))]);
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
