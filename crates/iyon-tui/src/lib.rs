mod app;
mod core;
mod util;

use anyhow::Result;
use app::App;
use core::terminal::InlineTerminal;
use ratatui::{TerminalOptions, Viewport};

pub fn run() -> Result<()> {
    let options = TerminalOptions {
        viewport: Viewport::Inline(20),
    };
    let terminal = ratatui::init_with_options(options);
    let result = {
        let mut terminal = InlineTerminal::new(terminal);
        App::default().run(&mut terminal)
    };

    if let Err(error) = crossterm::terminal::disable_raw_mode() {
        eprintln!("Failed to restore terminal: {error}");
    }

    result
}
