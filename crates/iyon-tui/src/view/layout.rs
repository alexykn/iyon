use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum CommitCapacityPolicy {
    #[default]
    SpillToTranscript,
    OccludeOnly,
}

#[derive(Debug, Clone)]
pub(crate) struct LayoutConfig {
    pub(crate) spacer_height: u16,
    pub(crate) chat_height: u16,
    pub(crate) active_height: u16,
    pub(crate) input_height: u16,
    pub(crate) info_height: u16,
    pub(crate) commit_capacity_policy: CommitCapacityPolicy,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            spacer_height: 0,
            chat_height: 1,
            active_height: 0,
            input_height: 3,
            info_height: 1,
            commit_capacity_policy: CommitCapacityPolicy::SpillToTranscript,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ComputedLayout {
    pub(crate) spacer_area: Rect,
    pub(crate) chat_area: Rect,
    pub(crate) active_area: Rect,
    pub(crate) input_area: Rect,
    pub(crate) info_area: Rect,
    pub(crate) visual_chat_capacity_rows: usize,
    pub(crate) commit_chat_capacity_rows: usize,
}

impl LayoutConfig {
    pub(crate) fn compute(&self, area: Rect) -> ComputedLayout {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(self.spacer_height),
                Constraint::Length(self.chat_height),
                Constraint::Length(self.active_height),
                Constraint::Length(self.input_height),
                Constraint::Length(self.info_height),
            ])
            .split(area);

        let visual_chat_capacity_rows = usize::from(vertical[1].height);
        let commit_chat_capacity_rows = match self.commit_capacity_policy {
            CommitCapacityPolicy::SpillToTranscript => visual_chat_capacity_rows,
            CommitCapacityPolicy::OccludeOnly => {
                visual_chat_capacity_rows.saturating_add(usize::from(vertical[2].height))
            }
        };

        ComputedLayout {
            spacer_area: vertical[0],
            chat_area: vertical[1],
            active_area: vertical[2],
            input_area: vertical[3],
            info_area: vertical[4],
            visual_chat_capacity_rows,
            commit_chat_capacity_rows,
        }
    }
}
