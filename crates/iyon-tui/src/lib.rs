//! Semantic terminal UI construction.
//!
//! [`View`] is an owned backend-neutral semantic value. Compose views with
//! [`View::horizontal`] and [`View::vertical`], style nodes with semantic
//! properties, and use [`IntoView`] to integrate application components.
//! Layout internals and terminal backend types remain private.
//!
//! ```compile_fail
//! use iyon_tui::presentation::ir::ViewKind;
//! ```
//!
//! ```compile_fail
//! use iyon_tui::{IntoView, View};
//!
//! let view = View::text("x").into_view();
//! let _ = view.kind;
//! ```
//!
//! ```compile_fail
//! use iyon_tui::{Decoration, RowChild, WidthRule};
//! ```
//!
//! ```compile_fail
//! use iyon_tui::View;
//!
//! let _ = View::text("x").container().no_wrap();
//! ```
//!
//! ```compile_fail
//! use iyon_tui::Horizontal;
//!
//! let _ = Horizontal::new();
//! ```

mod app;
mod component;
mod geometry;
mod input;
mod physical;
mod presentation;
mod runtime;
mod scrollback;
mod terminal;
mod theme;
mod tools;
mod transcript;
mod view;

pub use presentation::api::{
    BorderSpec, BorderStyle, ColorSpec, Horizontal, HorizontalAlign, Insets, IntoView,
    OverflowIndicator, StyleSpec, Text, TextAttribute, TextAttributeSpec, TextSpan, ThemeKey,
    Vertical, VerticalAlign, View, WrapMode,
};

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
