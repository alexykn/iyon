use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone)]
pub(crate) struct LayoutConfig {
    pub(crate) spacer_height: u16,
    pub(crate) chat_height: u16,
    pub(crate) input_height: u16,
    pub(crate) info_height: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            spacer_height: 0,
            chat_height: 1,
            input_height: 3,
            info_height: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ComputedLayout {
    pub(crate) spacer_area: Rect,
    pub(crate) chat_area: Rect,
    pub(crate) input_area: Rect,
    pub(crate) info_area: Rect,
}

impl LayoutConfig {
    pub(crate) fn compute(&self, area: Rect, config: &LayoutConfig) -> ComputedLayout {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(config.spacer_height),
                Constraint::Length(config.chat_height),
                Constraint::Length(config.input_height),
                Constraint::Length(config.info_height),
            ])
            .split(area);

        ComputedLayout {
            spacer_area: vertical[0],
            chat_area: vertical[1],
            input_area: vertical[2],
            info_area: vertical[3],
        }
    }
}
