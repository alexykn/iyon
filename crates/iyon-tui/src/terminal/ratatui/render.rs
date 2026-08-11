//! Ratatui terminal adapter for semantic View rendering.

use crate::{physical::Surface, terminal::ratatui::row::row_to_line};
use ratatui::{buffer::Buffer, layout::Rect};

pub(crate) fn render_surface(surface: &Surface, buffer: &mut Buffer, area: Rect) {
    for y in 0..area.height {
        clear_line(buffer, area.x, area.y.saturating_add(y), area.width);
    }
    for y in 0..surface.height().min(area.height) {
        let cells = (0..surface.width())
            .map(|x| surface.get(x, y).clone())
            .collect();
        let row = crate::physical::PhysicalRow::from_cells(cells);
        let y = area.y.saturating_add(y);
        buffer.set_line(area.x, y, &row_to_line(&row), area.width);
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
        clear_line(buffer, area.x, y, area.width);
        buffer.set_line(area.x, y, &row_to_line(row), area.width);
    }
}

/// `Buffer::set_line` does not clear cells after the line's last span. Reset
/// each row before lowering it so shrinking or moving retained content cannot
/// leave stale terminal cells behind.
fn clear_line(buffer: &mut Buffer, x: u16, y: u16, width: u16) {
    for offset in 0..width {
        if let Some(cell) = buffer.cell_mut((x.saturating_add(offset), y)) {
            cell.reset();
        }
    }
}
