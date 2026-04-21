use anyhow::Result;
use ratatui::{DefaultTerminal, layout::Rect, text::Line};

#[derive(Debug, Default)]
pub(crate) struct ScrollbackCommitter;

impl ScrollbackCommitter {
    pub(crate) fn commit_pending(
        &mut self,
        terminal: &mut DefaultTerminal,
        pending: &mut Vec<Line<'static>>,
    ) -> Result<usize> {
        if pending.is_empty() {
            return Ok(0);
        }

        let lines = pending.clone();
        let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);

        // PERF: prefer ratatui's scrolling-region insertion path for inline scrollback.
        // TODO(alxknt): verify behavior under tmux; if we observe regressions, add a
        // compatibility flag to disable scrolling-regions at runtime.
        terminal.insert_before(height, |buffer| {
            let width = buffer.area.width;
            for (row, line) in lines.iter().enumerate() {
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

        let committed = pending.len();
        pending.clear();
        Ok(committed)
    }
}
