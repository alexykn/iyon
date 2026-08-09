//! Ratatui lowering for backend-neutral physical rows.

use crate::physical::PhysicalRow;
use ratatui::text::{Line, Span};

use super::style::style;

pub(crate) fn row_to_line(row: &PhysicalRow) -> Line<'static> {
    let last_painted = row
        .cells()
        .iter()
        .rposition(|cell| cell.painted && !cell.continuation);
    let Some(last_painted) = last_painted else {
        return Line::from("");
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    for cell in &row.cells()[..=last_painted] {
        if cell.continuation {
            continue;
        }
        let text = cell.grapheme.clone().unwrap_or_else(|| " ".to_string());
        let cell_style = style(cell.style);
        if let Some(last) = spans.last_mut()
            && last.style == cell_style
        {
            last.content.to_mut().push_str(&text);
        } else {
            spans.push(Span::styled(text, cell_style));
        }
    }
    Line::from(spans)
}

pub(crate) fn rows_to_lines(rows: &[PhysicalRow]) -> Vec<Line<'static>> {
    rows.iter().map(row_to_line).collect()
}
