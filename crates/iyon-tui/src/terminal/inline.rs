//! INTERNAL PRESENTATION MECHANICS.
//!
//! Native terminal adapter. Feature code must not use this type directly.

use anyhow::Result;
use ratatui::{DefaultTerminal, Frame, layout::Rect, text::Line};

use crate::{
    backend::NativeHistorySink,
    physical::PhysicalRow,
    scene::PreparedSceneFrame,
    terminal::ratatui::{render_history_overlay, render_surface, rows_to_lines},
};

#[derive(Debug)]
pub(crate) struct InlineTerminal {
    terminal: DefaultTerminal,
    last_drawn_cursor: Option<(u16, u16)>,
}

pub(crate) struct InlineHistorySink<'a> {
    terminal: &'a mut InlineTerminal,
}

impl<'a> InlineHistorySink<'a> {
    pub(crate) fn new(terminal: &'a mut InlineTerminal) -> Self {
        Self { terminal }
    }

    pub(crate) fn viewport(&mut self) -> Result<crate::geometry::Size> {
        let area = self.terminal.current_viewport_area()?;
        Ok(crate::geometry::Size::new(area.width, area.height))
    }
}

impl NativeHistorySink for InlineHistorySink<'_> {
    type Error = anyhow::Error;

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
        let lines = rows_to_lines(rows);
        self.terminal.insert_history_rows(&lines)
    }
}

impl InlineTerminal {
    pub(crate) fn new(terminal: DefaultTerminal) -> Self {
        Self {
            terminal,
            last_drawn_cursor: None,
        }
    }

    pub(crate) fn draw<F>(&mut self, draw_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Frame),
    {
        self.terminal.draw(|frame| {
            draw_fn(frame);
        })?;

        self.last_drawn_cursor = self
            .terminal
            .get_cursor_position()
            .ok()
            .map(|position| (position.x, position.y));

        Ok(())
    }

    pub(crate) fn insert_history_rows(&mut self, rows: &[Line<'static>]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        let height = u16::try_from(rows.len()).unwrap_or(u16::MAX);
        let rendered_rows = usize::from(height);
        let rows = &rows[..rendered_rows];

        self.terminal.insert_before(height, |buffer| {
            let width = buffer.area.width;
            for (row, line) in rows.iter().enumerate() {
                let y = u16::try_from(row).unwrap_or(u16::MAX);
                let row_area = Rect {
                    x: buffer.area.x,
                    y: buffer.area.y + y,
                    width,
                    height: 1,
                };
                buffer.set_style(row_area, line.style);
                buffer.set_line(buffer.area.x, buffer.area.y + y, line, width);
            }
        })?;

        if let Some((x, y)) = self.last_drawn_cursor {
            self.terminal.set_cursor_position((x, y))?;
        }

        Ok(rendered_rows)
    }

    pub(crate) fn current_viewport_area(&mut self) -> Result<Rect> {
        self.terminal.autoresize()?;
        Ok(self.terminal.get_frame().area())
    }

    pub(crate) fn set_cursor_position(&mut self, position: (u16, u16)) -> Result<()> {
        self.terminal.set_cursor_position(position)?;
        self.last_drawn_cursor = Some(position);
        Ok(())
    }

    pub(crate) fn draw_scene_frame(&mut self, prepared: &PreparedSceneFrame) -> Result<()> {
        self.draw(|frame| {
            let area = frame.area();
            render_surface(&prepared.surface, frame.buffer_mut(), area);
            render_history_overlay(prepared.history_overlay.as_ref(), frame.buffer_mut(), area);
        })
    }

    pub(crate) fn position_after_final_frame(&mut self) -> Result<()> {
        let viewport = self.current_viewport_area()?;
        if viewport.height == 0 {
            return Ok(());
        }
        self.set_cursor_position((viewport.x, viewport.bottom().saturating_sub(1)))
    }
}
