use anyhow::{Context, Result, anyhow};
use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::terminal::TerminalEvent;

pub(crate) mod key;

pub(crate) fn map_event(event: Event) -> Option<TerminalEvent> {
    match event {
        Event::Key(event) => key::key_stroke(event).map(TerminalEvent::Key),
        Event::Paste(text) => Some(TerminalEvent::Paste(text)),
        Event::Resize(_, _) => Some(TerminalEvent::Resize),
        Event::FocusGained | Event::FocusLost | Event::Mouse(_) => None,
    }
}

pub(crate) fn setup() -> Result<()> {
    enable_raw_mode().context("enable terminal raw mode")?;
    if let Err(error) = execute!(std::io::stdout(), EnableBracketedPaste) {
        return match disable_raw_mode() {
            Ok(()) => Err(anyhow!(error).context("enable bracketed paste")),
            Err(disable_error) => Err(anyhow!(error)
                .context("enable bracketed paste")
                .context(format!("also failed to disable raw mode: {disable_error}"))),
        };
    }
    Ok(())
}

pub(crate) fn restore() -> Result<()> {
    let mut first_error = None;
    if let Err(error) =
        execute!(std::io::stdout(), DisableBracketedPaste).context("disable bracketed paste")
    {
        first_error = Some(error);
    }
    if let Err(error) = disable_raw_mode().context("disable terminal raw mode")
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    first_error.map_or(Ok(()), Err)
}
