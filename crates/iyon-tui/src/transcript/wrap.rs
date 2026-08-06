use std::ops::Range;

use ratatui::text::{Line, Span};

use crate::input::wrap::{WrapConfig, wrap_ranges};
use crate::transcript::row::{TranscriptRow, compute_indent};

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
    // Derive the hanging-indent geometry once. `content_width` already accounts
    // for the full left prefix (outer + nesting + marker gutter) and the right
    // margin, so every physical row fits within `width` after prefixes are
    // applied. First line carries the marker prefix; continuations align to the
    // content edge (equal-width prefix of spaces).
    let indent = compute_indent(&row.layout, usize::from(width));
    let content_width = indent.content_width.max(1);

    let flattened = flatten_line_from_boundary(&row.line, start);
    let wrapped_ranges = wrap_ranges(
        &flattened.text,
        WrapConfig::transcript(content_width as u16),
    );
    let is_first_physical = start.span_index == 0 && start.byte_offset == 0;

    for (index, range) in wrapped_ranges.into_iter().enumerate() {
        let mut physical = line_from_flat_range(&row.line, &flattened.segments, range.clone());
        let prefix = if index == 0 && is_first_physical {
            &indent.first_prefix
        } else {
            &indent.continuation_prefix
        };
        if !physical.spans.is_empty() {
            physical = prepend_prefix(physical, row.style, prefix);
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

/// Prepends the gutter/prefix to a physical row's first span. When the content's
/// first span shares the gutter style, the prefix is merged into it to emit a
/// single styled span; otherwise it is inserted as its own leading span.
fn prepend_prefix(
    mut line: Line<'static>,
    style: ratatui::style::Style,
    prefix: &str,
) -> Line<'static> {
    if prefix.is_empty() || line.spans.is_empty() {
        return line;
    }
    if let Some(first) = line.spans.first_mut()
        && first.style == style
    {
        let mut text = prefix.to_string();
        text.push_str(first.content.as_ref());
        first.content = text.into();
        return line;
    }
    line.spans
        .insert(0, Span::styled(prefix.to_string(), style));
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
    use unicode_width::UnicodeWidthStr;

    use super::{TranscriptCommitBoundary, wrap_transcript_rows};
    use crate::transcript::row::{Marker, TranscriptRow, compute_indent};

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
        let margin = crate::transcript::row::RowLayout::content();
        let row = TranscriptRow {
            line: Line::from("abcdefgh"),
            style: Line::from("").style,
            layout: margin,
        };
        // width 6 minus outer left(2) - right(2) = content width 2.
        let rows = wrap_transcript_rows(6, &[row], TranscriptCommitBoundary::default());

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["  ab", "  cd", "  ef", "  gh"]);
    }

    #[test]
    fn empty_content_row_stays_blank_despite_margin() {
        let row = TranscriptRow {
            line: Line::from(""),
            style: Line::from("").style,
            layout: crate::transcript::row::RowLayout::content(),
        };
        let rows = wrap_transcript_rows(6, &[row], TranscriptCommitBoundary::default());

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, [""]);
    }

    #[test]
    fn bulleted_row_alignment_follows_text_not_bullet_dot() {
        // Tool bullet: `● ` (2 cols) + outer left (2) = first prefix 4 wide;
        // content width = 8 - 4 - right(2) = 2.
        let row = TranscriptRow::bullet("abcdefgh", Line::from("").style);
        let rows = wrap_transcript_rows(8, &[row], TranscriptCommitBoundary::default());

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["  ● ab", "    cd", "    ef", "    gh"]);
    }

    #[test]
    fn symmetric_margin_shrinks_content_width() {
        let row = TranscriptRow::body("abcdefgh", Line::from("").style);
        // width 6 - left 2 - right 2 = 2.
        let rows = wrap_transcript_rows(6, &[row], TranscriptCommitBoundary::default());

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        assert_eq!(text_rows, ["  ab", "  cd", "  ef", "  gh"]);
    }

    #[test]
    fn nested_bullet_continuation_aligns_to_item_content() {
        // Nested (depth 1) bullet: outer 2 + nesting 2 + marker 2 = first prefix
        // 6 wide; continuation is the content edge (6 columns of spaces).
        let row = TranscriptRow::markdown_unordered(
            Line::styled("abcdefgh", Line::from("").style),
            Line::from("").style,
            1,
        );
        let rows = wrap_transcript_rows(12, &[row], TranscriptCommitBoundary::default());

        let text_rows = rows.rows.iter().map(line_text).collect::<Vec<_>>();
        // width 12 - first(6) - right(2) = 4 content cols.
        assert_eq!(text_rows, ["    • abcd", "      efgh"]);
    }

    #[test]
    fn ordered_marker_right_aligns_single_against_double_digit() {
        // Shared digit_width = 2 (widest is 10). "10. " is 4 wide; "9." is
        // right-aligned within the same 2-digit gutter -> " 9. " (also 4 wide).
        let single = TranscriptRow::markdown_ordered(
            Line::styled("x", Line::from("").style),
            Line::from("").style,
            9,
            2,
            0,
        );
        let double = TranscriptRow::markdown_ordered(
            Line::styled("y", Line::from("").style),
            Line::from("").style,
            10,
            2,
            0,
        );
        assert_eq!(single.layout.marker.as_ref().unwrap().text(), " 9. ");
        assert_eq!(double.layout.marker.as_ref().unwrap().text(), "10. ");
    }

    #[test]
    fn tool_result_aligns_with_tool_call_text() {
        // Tool-call text and tool-result text must share the same content column
        // (outer 2 + tool gutter 2 = col 4), so results read as a continuation of
        // the call rather than drifting back to the agent-text column.
        let call = TranscriptRow::bullet("bash run", Line::from("").style);
        let result = TranscriptRow::tool_result("output line", Line::from("").style);
        assert_eq!(call.layout.content_column(), 4);
        assert_eq!(result.layout.content_column(), 4);

        // Wrapped, both first lines start with the same 4-column prefix and the
        // full text fits on the first line (content width 20-4-2=14).
        let call_rows = wrap_transcript_rows(20, &[call], TranscriptCommitBoundary::default());
        let result_rows = wrap_transcript_rows(20, &[result], TranscriptCommitBoundary::default());
        assert_eq!(line_text(&call_rows.rows[0]), "  ● bash run");
        assert_eq!(line_text(&result_rows.rows[0]), "    output line");
    }

    #[test]
    fn compute_indent_prefixes_share_display_width() {
        for layout in [
            crate::transcript::row::RowLayout::content(),
            crate::transcript::row::RowLayout {
                outer: crate::transcript::row::Insets::symmetric(2),
                nesting_depth: 1,
                marker: Some(Marker::Bullet),
                marker_gutter_width: 2,
            },
            crate::transcript::row::RowLayout {
                outer: crate::transcript::row::Insets::symmetric(2),
                nesting_depth: 0,
                marker: Some(Marker::Ordered {
                    index: 10,
                    digit_width: 2,
                }),
                marker_gutter_width: 4,
            },
        ] {
            let indent = compute_indent(&layout, 80);
            assert_eq!(
                indent.first_prefix.width(),
                indent.continuation_prefix.width(),
                "prefixes must share display width"
            );
        }
    }

    #[test]
    fn physical_rows_never_exceed_viewport_width() {
        // Property-style: for a variety of layouts and widths, every produced
        // physical row must be no wider than the viewport (prefix + content fits).
        // Rows whose prefix alone is wider than the viewport are degenerate
        // (impossible to honour), so we skip those and assert the feasible domain.
        let rows = [
            TranscriptRow::body("a fairly long line of text", Line::from("").style),
            TranscriptRow::bullet("another long tool line", Line::from("").style),
            TranscriptRow::markdown_unordered(
                Line::styled("nested item text", Line::from("").style),
                Line::from("").style,
                2,
            ),
            TranscriptRow::markdown_ordered(
                Line::styled("ordered text", Line::from("").style),
                Line::from("").style,
                9,
                2,
                1,
            ),
            TranscriptRow::new(Line::from("plain col-0")),
        ];

        for width in [6u16, 8, 12, 20] {
            for row in &rows {
                let indent = compute_indent(&row.layout, usize::from(width));
                if indent.first_prefix.width() + 1 > usize::from(width) {
                    // Prefix alone consumes the whole viewport (no room for even a
                    // single content column): degenerate, skip.
                    continue;
                }
                let wrapped = wrap_transcript_rows(
                    width,
                    std::slice::from_ref(row),
                    TranscriptCommitBoundary::default(),
                );
                for physical in &wrapped.rows {
                    let w = physical.width();
                    assert!(
                        w <= usize::from(width),
                        "row {w} exceeds width {width}: {:?}",
                        line_text(physical)
                    );
                }
            }
        }
    }
}
