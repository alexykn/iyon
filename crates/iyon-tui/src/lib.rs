mod app;
mod input;
mod runtime;
mod scrollback;
mod terminal;
mod theme;
mod tools;
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
use runtime::BackendEventHandler;
use terminal::InlineTerminal;

pub fn run() -> Result<()> {
    run_with_backend_handler(BackendEventHandler::default())
}

pub fn run_with_core(core: iyon_core::IyonCore) -> Result<()> {
    run_with_backend_handler(BackendEventHandler::new(core))
}

fn run_with_backend_handler(backend_handler: BackendEventHandler) -> Result<()> {
    execute!(stdout(), EnableBracketedPaste)?;

    let options = TerminalOptions {
        viewport: Viewport::Inline(20),
    };
    let terminal = ratatui::init_with_options(options);
    execute!(stdout(), EnableBracketedPaste)?;
    let result = {
        let mut terminal = InlineTerminal::new(terminal);
        App::with_backend_handler(backend_handler).run(&mut terminal)
    };

    if let Err(error) = execute!(stdout(), DisableBracketedPaste) {
        eprintln!("Failed to disable bracketed paste: {error}");
    }

    if let Err(error) = crossterm::terminal::disable_raw_mode() {
        eprintln!("Failed to restore terminal: {error}");
    }

    result
}
