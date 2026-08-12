use crate::{Key, KeyStroke, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollCommand {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Start,
    End,
}

pub(crate) fn map_scroll_key(key: KeyStroke) -> Option<ScrollCommand> {
    if key.modifiers() != Modifiers::NONE {
        return None;
    }
    match key.key() {
        Key::Up => Some(ScrollCommand::LineUp),
        Key::Down => Some(ScrollCommand::LineDown),
        Key::PageUp => Some(ScrollCommand::PageUp),
        Key::PageDown => Some(ScrollCommand::PageDown),
        Key::Home => Some(ScrollCommand::Start),
        Key::End => Some(ScrollCommand::End),
        _ => None,
    }
}
