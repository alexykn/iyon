pub(crate) mod backend;
pub(crate) mod crossterm;
pub(crate) mod inline;
pub(crate) mod ratatui;

pub(crate) use backend::{TerminalBackend, TerminalEvent};

pub(crate) fn enter_default() -> anyhow::Result<impl TerminalBackend> {
    crossterm::CrosstermRatatuiBackend::enter()
}
pub(crate) use inline::{InlineHistorySink, InlineTerminal};
