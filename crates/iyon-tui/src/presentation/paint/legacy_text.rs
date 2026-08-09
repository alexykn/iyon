//! Ordinary text measurement, wrapping, and physical-cell construction.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    physical::{PhysicalCell, PhysicalRow, PhysicalStyle},
    presentation::{
        WidthRule,
        ir::{TextCursorAnchor, TextView},
        wrap::{StyledGrapheme, WrappedLine, styled_hard_lines, wrap_styled_lines},
    },
};

use crate::presentation::layout::ViewCompiler;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledTextRow {
    pub(crate) row: PhysicalRow,
    pub(crate) source_end: Option<usize>,
    pub(crate) cursor_column: Option<usize>,
    pub(crate) fits: bool,
    pub(crate) width: usize,
}

impl ViewCompiler {
    pub(crate) fn compile_text_with_metadata(
        &self,
        text: &TextView,
        max_width: u16,
        width_rule: WidthRule,
        inherited: PhysicalStyle,
        track_source: bool,
    ) -> (u16, Vec<CompiledTextRow>) {
        let mut relative_source = 0usize;
        let spans = text.spans.iter().map(|span| {
            let base = if track_source {
                let current = relative_source;
                relative_source += span.text.len();
                Some(current)
            } else {
                None
            };
            (
                span.text.as_str(),
                self.theme.resolve_text_style(inherited, &span.style),
                base,
            )
        });
        let hard_lines = styled_hard_lines(spans);
        let intrinsic_width = hard_lines
            .iter()
            .map(|line| line.iter().map(|grapheme| grapheme.width).sum::<usize>())
            .max()
            .unwrap_or(0);
        let source = text.cursor.map(|anchor| {
            let source = text
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>();
            validate_cursor_anchor(&source, anchor);
            source
        });
        let cursor_needs_cell = text.cursor.is_some_and(|anchor| {
            !hard_lines.iter().flatten().any(|grapheme| {
                grapheme
                    .source
                    .as_ref()
                    .is_some_and(|range| range.start == anchor.byte_offset)
            })
        });
        let intrinsic_width = intrinsic_width + usize::from(cursor_needs_cell);
        let width = match width_rule {
            WidthRule::Fit => intrinsic_width.min(usize::from(max_width)) as u16,
            WidthRule::Fill => max_width,
        };
        let mut wrapped = if let Some(source) = source.as_deref()
            && text.cursor.is_some()
        {
            crate::presentation::wrap::wrap_input_styled_lines(&hard_lines, source, width)
        } else {
            wrap_styled_lines(&hard_lines, width, text.wrap)
        };
        let cursor_marker = text.cursor.and_then(|anchor| {
            source
                .as_deref()
                .map(|source| cursor_position(source, anchor, usize::from(width), &mut wrapped))
        });
        let rows = wrapped
            .into_iter()
            .enumerate()
            .map(|(row_index, w_line)| CompiledTextRow {
                row: row_from_graphemes(&w_line.graphemes),
                source_end: w_line
                    .graphemes
                    .last()
                    .and_then(|g| g.source.as_ref())
                    .map(|r| r.end),
                cursor_column: cursor_marker
                    .and_then(|(cursor_row, column)| (cursor_row == row_index).then_some(column)),
                fits: w_line.fits,
                width: w_line.width,
            })
            .collect();
        (width, rows)
    }
}

fn validate_cursor_anchor(text: &str, anchor: TextCursorAnchor) {
    assert!(
        anchor.byte_offset <= text.len(),
        "text cursor anchor exceeds source length"
    );
    assert!(
        text.is_char_boundary(anchor.byte_offset),
        "text cursor anchor is not a UTF-8 boundary"
    );
}

fn cursor_position(
    source: &str,
    anchor: TextCursorAnchor,
    max_columns: usize,
    rows: &mut Vec<WrappedLine<'_>>,
) -> (usize, usize) {
    let max_columns = max_columns.max(1);
    let ranges = crate::presentation::wrap::input_wrap_ranges(source, max_columns as u16);
    let mut row = ranges
        .partition_point(|range| range.start <= anchor.byte_offset)
        .saturating_sub(1);
    let line = &ranges[row];
    let mut column = UnicodeWidthStr::width(&source[line.start..anchor.byte_offset]);
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

pub(crate) fn row_from_string(text: &str, style: PhysicalStyle) -> PhysicalRow {
    let graphemes = text
        .graphemes(true)
        .map(|text| StyledGrapheme {
            text: Cow::Owned(text.to_string()),
            width: UnicodeWidthStr::width(text),
            style,
            source: None,
        })
        .collect::<Vec<_>>();
    row_from_graphemes(&graphemes)
}

pub(crate) fn row_from_graphemes(graphemes: &[StyledGrapheme<'_>]) -> PhysicalRow {
    let mut cells = Vec::new();
    for grapheme in graphemes {
        if grapheme.width == 0 {
            continue;
        }
        cells.push(PhysicalCell {
            grapheme: Some(grapheme.text.to_string()),
            style: grapheme.style,
            painted: true,
            continuation: false,
        });
        for _ in 1..grapheme.width {
            cells.push(PhysicalCell {
                grapheme: None,
                style: grapheme.style,
                painted: true,
                continuation: true,
            });
        }
    }
    PhysicalRow::from_cells(cells)
}
