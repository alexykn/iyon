use anyhow::{Context, Result};
use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste, Event, EventStream},
    execute,
};
use futures_util::StreamExt;
use ratatui::{TerminalOptions, Viewport};

use crate::{
    backend::NativeHistorySink,
    geometry::Size,
    physical::PhysicalRow,
    scene::PreparedSceneFrame,
    terminal::{InlineHistorySink, InlineTerminal, TerminalBackend, TerminalEvent},
};

/// The current terminal adapter. Crossterm and Ratatui are confined here.
pub(crate) struct CrosstermRatatuiBackend {
    terminal: InlineTerminal,
    events: EventStream,
    restored: bool,
}

impl CrosstermRatatuiBackend {
    pub(crate) fn enter() -> Result<Self> {
        let terminal = ratatui::try_init_with_options(TerminalOptions {
            viewport: Viewport::Inline(20),
        })
        .context("initialize inline terminal")?;

        if let Err(error) = execute!(std::io::stdout(), EnableBracketedPaste) {
            ratatui::restore();
            return Err(error).context("enable bracketed paste");
        }

        Ok(Self {
            terminal: InlineTerminal::new(terminal),
            events: EventStream::new(),
            restored: false,
        })
    }
}

impl NativeHistorySink for CrosstermRatatuiBackend {
    type Error = anyhow::Error;

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
        InlineHistorySink::new(&mut self.terminal).insert_history_rows(rows)
    }
}

impl TerminalBackend for CrosstermRatatuiBackend {
    async fn next_event(&mut self) -> Result<TerminalEvent> {
        loop {
            let event = self
                .events
                .next()
                .await
                .context("terminal event stream closed")??;
            match event {
                Event::Key(event) => {
                    if let Some(key) = super::key::key_stroke(event) {
                        return Ok(TerminalEvent::Key(key));
                    }
                }
                Event::Paste(text) => return Ok(TerminalEvent::Paste(text)),
                Event::Resize(_, _) => return Ok(TerminalEvent::Resize),
                Event::FocusGained | Event::FocusLost | Event::Mouse(_) => {}
            }
        }
    }

    fn viewport(&mut self) -> Result<Size> {
        InlineHistorySink::new(&mut self.terminal).viewport()
    }

    fn draw_frame(&mut self, frame: &PreparedSceneFrame) -> Result<()> {
        self.terminal.draw_scene_frame(frame)
    }

    fn position_after_final_frame(&mut self) -> Result<()> {
        self.terminal.position_after_final_frame()
    }

    fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;

        let disable_paste =
            execute!(std::io::stdout(), DisableBracketedPaste).context("disable bracketed paste");
        let restore_terminal = ratatui::try_restore().context("restore terminal");
        disable_paste.and(restore_terminal)
    }
}

impl Drop for CrosstermRatatuiBackend {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
