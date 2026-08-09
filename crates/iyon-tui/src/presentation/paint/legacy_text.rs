//! Ordinary text measurement, wrapping, and physical-cell construction.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    physical::{PhysicalCell, PhysicalRow, PhysicalStyle},
    presentation::{
        WidthRule,
        ir::TextView,
        wrap::{StyledGrapheme, styled_hard_lines, text_flow},
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
        let source = text.cursor.map(|anchor| {
            let source = text
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>();
            validate_cursor_anchor(&source, anchor);
            source
        });
        let flow = text_flow(text, hard_lines, source.as_deref(), max_width, width_rule);
        let crate::presentation::wrap::StyledTextFlow {
            width: _width,
            rows: flow_rows,
            cursor,
        } = flow;
        let rows = flow_rows
            .into_iter()
            .enumerate()
            .map(|(row_index, w_line)| CompiledTextRow {
                row: row_from_graphemes(&w_line.graphemes),
                source_end: w_line
                    .graphemes
                    .last()
                    .and_then(|g| g.source.as_ref())
                    .map(|r| r.end),
                cursor_column: cursor
                    .and_then(|(cursor_row, column)| (cursor_row == row_index).then_some(column)),
                fits: w_line.fits,
                width: w_line.width,
            })
            .collect();
        (flow.width, rows)
    }
}

fn validate_cursor_anchor(text: &str, anchor: crate::presentation::ir::TextCursorAnchor) {
    assert!(
        anchor.byte_offset <= text.len(),
        "text cursor anchor exceeds source length"
    );
    assert!(
        text.is_char_boundary(anchor.byte_offset),
        "text cursor anchor is not a UTF-8 boundary"
    );
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
