mod app;
mod core;
mod util;

use anyhow::Result;
use app::App;
use ratatui::{TerminalOptions, Viewport};

pub fn run() -> Result<()> {
    let options = TerminalOptions {
        viewport: Viewport::Inline(20),
    };
    let mut terminal = ratatui::init_with_options(options);
    let result = App::default().run(&mut terminal);
    ratatui::restore();
    result
}
