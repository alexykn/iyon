use ratatui::text::Line;

use crate::{
    core::input::InputBuffer,
    util::format::{TimelineItem, TuiFormatter},
};

#[derive(Debug, Default)]
pub(crate) struct AppState {
    pub(crate) input: InputBuffer,
    pub(crate) chat: ChatState,
    pub(crate) info: InfoState,
    pub(crate) pending_scrollback: Vec<Line<'static>>,
    pub(crate) input_content_width: u16,
    pub(crate) scroll: u16,
    pub(crate) exit: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatState {
    pub(crate) lines: Vec<Line<'static>>,
    formatter: TuiFormatter,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            formatter: TuiFormatter,
        }
    }
}

impl ChatState {
    pub(crate) fn push_item(&mut self, item: TimelineItem) {
        if !self.lines.is_empty() {
            self.lines.push(Line::from(""));
        }
        self.lines.extend(self.formatter.format(&item));
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InfoState {
    pub(crate) status: String,
}
