//! Ratatui terminal adapter for semantic View rendering.

use crate::{
    presentation::{View, layout::compile_view},
    terminal::ratatui::row::row_to_line,
};
use ratatui::{buffer::Buffer, layout::Rect};

pub(crate) fn render_view(view: &View, buffer: &mut Buffer, area: Rect) {
    let rows = compile_view(view, area.width).rows;
    for (index, row) in rows.iter().take(usize::from(area.height)).enumerate() {
        let y = area.y.saturating_add(index as u16);
        let line = row_to_line(row);
        buffer.set_line(area.x, y, &line, area.width);
    }
}
