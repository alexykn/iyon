//! Projected-text stream lowering.

use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

use crate::{
    physical::{PhysicalRow, PhysicalStyle},
    presentation::{
        ir::WidthRule,
        layout::{CompiledTextRow, ViewCompiler, row_from_graphemes, row_from_string},
        paint::StyleContext,
        wrap::{StyledGrapheme, wrap_styled_lines},
    },
    stream::{ProjectedText, ProjectedTextLayout, projected_atoms},
};

impl ViewCompiler {
    pub(crate) fn compile_projected_text_with_metadata(
        &self,
        text: &ProjectedText,
        max_width: u16,
    ) -> (u16, Vec<CompiledTextRow>) {
        self.compile_projected_text_with_style(text, max_width, PhysicalStyle::default())
    }

    fn compile_projected_text_with_style(
        &self,
        text: &ProjectedText,
        max_width: u16,
        inherited: PhysicalStyle,
    ) -> (u16, Vec<CompiledTextRow>) {
        if let ProjectedTextLayout::Hanging {
            body_column,
            prefix,
            prefix_style,
            show_prefix,
            ..
        } = &text.layout
        {
            let body_start = text
                .runs
                .first()
                .map_or(text.content_range.end, |run| run.owned.start);
            let body = ProjectedText {
                content_range: crate::stream::StreamRange::new(body_start, text.content_range.end),
                terminator: text.terminator,
                width: WidthRule::Fill,
                wrap: text.wrap,
                align: text.align,
                layout: ProjectedTextLayout::Plain,
                runs: text.runs.clone(),
            };
            let body_width = max_width.saturating_sub(*body_column).max(1);
            let (_, body_rows) =
                self.compile_projected_text_with_style(&body, body_width, inherited);
            let prefix_width = UnicodeWidthStr::width(prefix.as_str());
            let mut rows = Vec::with_capacity(body_rows.len());
            for (index, mut row) in body_rows.into_iter().enumerate() {
                let indent = if index == 0 && *show_prefix {
                    prefix.clone()
                } else {
                    " ".repeat(usize::from(*body_column))
                };
                let prefix_style = if index == 0 && *show_prefix {
                    self.theme
                        .resolve_text_style(inherited, prefix_style, &StyleContext::default())
                } else {
                    inherited
                };
                let prefix_row = row_from_string(&indent, prefix_style);
                let mut cells = prefix_row.cells().to_vec();
                cells.extend_from_slice(row.row.cells());
                row.row = PhysicalRow::from_cells(cells);
                row.width = row.width.saturating_add(if index == 0 && *show_prefix {
                    prefix_width
                } else {
                    usize::from(*body_column)
                });
                row.fits = row.width <= usize::from(max_width);
                row.source_end = row.source_end.map(|end| {
                    end + (body_start.as_u64() - text.content_range.start.as_u64()) as usize
                });
                rows.push(row);
            }
            return (max_width, rows);
        }

        let hard_lines = projected_hard_lines(&self.theme, text, inherited);
        let intrinsic_width = hard_lines
            .iter()
            .map(|line| line.iter().map(|atom| atom.width).sum::<usize>())
            .max()
            .unwrap_or(0);
        let width = match text.width {
            WidthRule::Fit => intrinsic_width.min(usize::from(max_width)) as u16,
            WidthRule::Fill => max_width,
        };
        let rows = wrap_styled_lines(&hard_lines, width, text.wrap)
            .into_iter()
            .map(|w_line| CompiledTextRow {
                row: row_from_graphemes(&w_line.graphemes),
                source_end: w_line
                    .graphemes
                    .last()
                    .and_then(|g| g.source.as_ref())
                    .map(|r| r.end),
                cursor_column: None,
                fits: w_line.fits,
                width: w_line.width,
            })
            .collect();
        (width, rows)
    }
}

fn projected_hard_lines(
    theme: &crate::presentation::paint::ThemeResolver,
    text: &ProjectedText,
    inherited: PhysicalStyle,
) -> Vec<Vec<StyledGrapheme<'static>>> {
    let mut hard_lines = vec![Vec::new()];
    for atom in projected_atoms(text) {
        let mapped = StyledGrapheme {
            text: Cow::Owned(atom.display.clone()),
            width: UnicodeWidthStr::width(atom.display.as_str()),
            style: theme.resolve_text_style(inherited, &atom.style, &StyleContext::default()),
            source: Some(
                (atom.owned.start.as_u64() - text.content_range.start.as_u64()) as usize
                    ..(atom.owned.end.as_u64() - text.content_range.start.as_u64()) as usize,
            ),
        };
        if atom.display == "\n" {
            hard_lines.push(Vec::new());
        } else {
            hard_lines
                .last_mut()
                .expect("stream has a hard line")
                .push(mapped);
        }
    }
    hard_lines
}
