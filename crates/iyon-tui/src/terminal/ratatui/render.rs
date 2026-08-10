//! Ratatui terminal adapter for semantic View rendering.

use crate::{physical::Surface, terminal::ratatui::row::row_to_line};
use ratatui::{buffer::Buffer, layout::Rect};

pub(crate) fn render_surface(surface: &Surface, buffer: &mut Buffer, area: Rect) {
    for y in 0..surface.height().min(area.height) {
        let cells = (0..surface.width())
            .map(|x| surface.get(x, y).clone())
            .collect();
        let row = crate::physical::PhysicalRow::from_cells(cells);
        buffer.set_line(
            area.x,
            area.y.saturating_add(y),
            &row_to_line(&row),
            area.width,
        );
    }
}

pub(crate) fn render_history_overlay(
    overlay: Option<&crate::history::HistoryPhysicalOverlay>,
    buffer: &mut Buffer,
    area: Rect,
) {
    let Some(overlay) = overlay else {
        return;
    };
    for (index, row) in overlay.rows.iter().enumerate() {
        let Some(y) = area
            .y
            .checked_add(overlay.row)
            .and_then(|base| base.checked_add(u16::try_from(index).ok()?))
        else {
            continue;
        };
        if y >= area.bottom() {
            continue;
        }
        buffer.set_line(area.x, y, &row_to_line(row), area.width);
    }
}
