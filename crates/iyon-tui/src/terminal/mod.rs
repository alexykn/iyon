pub(crate) mod backend;
pub(crate) mod termwiz;

pub(crate) use backend::{PresentReceipt, TerminalBackend, TerminalEvent};

pub(crate) fn enter_default() -> anyhow::Result<impl TerminalBackend> {
    termwiz::TermwizBackend::enter()
}
