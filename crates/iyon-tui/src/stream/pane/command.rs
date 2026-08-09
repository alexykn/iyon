use super::{StreamPane, StreamingSource};
use crate::{EventCx, InteractionResult, Key, KeyStroke, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamPaneCommand {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Start,
    End,
}

impl<S: StreamingSource> StreamPane<S> {
    pub(super) fn map_command(&self, key: KeyStroke) -> Option<StreamPaneCommand> {
        if key.modifiers() != Modifiers::NONE {
            return None;
        }
        match key.key() {
            Key::Up => Some(StreamPaneCommand::LineUp),
            Key::Down => Some(StreamPaneCommand::LineDown),
            Key::PageUp => Some(StreamPaneCommand::PageUp),
            Key::PageDown => Some(StreamPaneCommand::PageDown),
            Key::Home => Some(StreamPaneCommand::Start),
            Key::End => Some(StreamPaneCommand::End),
            _ => None,
        }
    }

    pub(super) fn handle_command(
        &mut self,
        command: StreamPaneCommand,
        _cx: &mut EventCx<'_>,
    ) -> InteractionResult {
        match command {
            StreamPaneCommand::LineUp => {
                self.scroll_up(1);
            }
            StreamPaneCommand::LineDown => {
                self.scroll_down(1);
            }
            StreamPaneCommand::PageUp => {
                self.page_up();
            }
            StreamPaneCommand::PageDown => {
                self.page_down();
            }
            StreamPaneCommand::Start => self.scroll_to_start(),
            StreamPaneCommand::End => self.follow_end(),
        }
        InteractionResult::Consumed
    }
}
