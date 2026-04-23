use anyhow::Result;
use ratatui::{DefaultTerminal, Frame, layout::Rect, text::Line};

#[derive(Debug)]
pub(crate) struct InlineTerminal {
    terminal: DefaultTerminal,
    last_viewport_area: Option<Rect>,
    invalidate_next_draw: bool,
}

impl InlineTerminal {
    pub(crate) fn new(terminal: DefaultTerminal) -> Self {
        Self {
            terminal,
            last_viewport_area: None,
            invalidate_next_draw: false,
        }
    }

    pub(crate) fn draw<F>(&mut self, draw_fn: F) -> Result<()>
    where
        F: FnOnce(&mut Frame),
    {
        if self.invalidate_next_draw {
            self.terminal.clear()?;
            self.invalidate_next_draw = false;
        }

        let mut area = None;
        self.terminal.draw(|frame| {
            area = Some(frame.area());
            draw_fn(frame);
        })?;
        self.last_viewport_area = area;
        Ok(())
    }

    pub(crate) fn insert_history_rows(&mut self, rows: &[Line<'static>]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        let height = u16::try_from(rows.len()).unwrap_or(u16::MAX);
        let rendered_rows = usize::from(height);
        let rows = &rows[..rendered_rows];

        let cursor_before_insert = self.terminal.get_cursor_position().ok();

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

        if let Some(cursor_position) = cursor_before_insert {
            self.terminal.set_cursor_position(cursor_position)?;
        }

        Ok(rendered_rows)
    }

    pub(crate) fn last_viewport_area(&self) -> Option<Rect> {
        self.last_viewport_area
    }

    pub(crate) fn invalidate_next_draw(&mut self) {
        self.invalidate_next_draw = true;
    }

    pub(crate) fn size(&mut self) -> Result<Rect> {
        let size = self.terminal.size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }

    pub(crate) fn set_cursor_position(&mut self, position: (u16, u16)) -> Result<()> {
        self.terminal.set_cursor_position(position)?;
        Ok(())
    }
}
