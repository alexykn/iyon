mod app;
mod input;
mod runtime;
mod scrollback;
mod terminal;
mod transcript;
mod view;

use std::io::stdout;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
};
use ratatui::{TerminalOptions, Viewport};
use terminal::InlineTerminal;

pub fn run() -> Result<()> {
    execute!(stdout(), EnableBracketedPaste)?;

    let options = TerminalOptions {
        viewport: Viewport::Inline(20),
    };
    let terminal = ratatui::init_with_options(options);
    execute!(stdout(), EnableBracketedPaste)?;
    let result = {
        let mut terminal = InlineTerminal::new(terminal);
        App::default().run(&mut terminal)
    };

    if let Err(error) = execute!(stdout(), DisableBracketedPaste) {
        eprintln!("Failed to disable bracketed paste: {error}");
    }

    if let Err(error) = crossterm::terminal::disable_raw_mode() {
        eprintln!("Failed to restore terminal: {error}");
    }

    result
}
