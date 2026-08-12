use super::{StreamPane, StreamingSource};
use crate::{
    EventCx, InteractionResult, KeyStroke,
    scroll_command::{self, ScrollCommand},
};

impl<S: StreamingSource> StreamPane<S> {
    pub(super) fn map_command(&self, key: KeyStroke) -> Option<ScrollCommand> {
        scroll_command::map_scroll_key(key)
    }

    pub(super) fn handle_command(
        &mut self,
        command: ScrollCommand,
        _cx: &mut EventCx<'_>,
    ) -> InteractionResult {
        match command {
            ScrollCommand::LineUp => {
                self.scroll_up(1);
            }
            ScrollCommand::LineDown => {
                self.scroll_down(1);
            }
            ScrollCommand::PageUp => {
                self.page_up();
            }
            ScrollCommand::PageDown => {
                self.page_down();
            }
            ScrollCommand::Start => self.scroll_to_start(),
            ScrollCommand::End => self.follow_end(),
        }
        InteractionResult::Consumed
    }
}
