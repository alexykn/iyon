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
    pub(crate) panel_height: u16,
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
            panel_height: 0,
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
    pub(crate) panel_area: Rect,
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
                // Chat is bounded-flexible: it hugs its content height when there is
                // room, but acts as the overflow absorber so the fixed regions below it
                // (active / panel / input / info) never get squeezed below the heights
                // they need (e.g. the composer must stay tall enough for its cursor).
                Constraint::Max(self.chat_height),
                Constraint::Length(self.active_height),
                Constraint::Length(self.panel_height),
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
            panel_area: vertical[3],
            input_area: vertical[4],
            info_area: vertical[5],
            visual_chat_capacity_rows,
            commit_chat_capacity_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(h: u16) -> Rect {
        Rect::new(0, 0, 80, h)
    }

    #[test]
    fn panel_height_zero_has_no_effect() {
        let cfg = LayoutConfig::default();
        let layout = cfg.compute(rect(30));
        assert_eq!(layout.panel_area.height, 0);
    }

    #[test]
    fn panel_region_reserves_height_and_shrinks_neighbors() {
        let cfg = LayoutConfig {
            chat_height: 10,
            active_height: 5,
            panel_height: 3,
            input_height: 3,
            info_height: 1,
            ..LayoutConfig::default()
        };
        let layout = cfg.compute(rect(30));
        assert_eq!(layout.panel_area.height, 3);
        // The panel sits between active and input in vertical order.
        assert!(layout.active_area.bottom() <= layout.panel_area.y);
        assert!(layout.panel_area.bottom() <= layout.input_area.y);
    }

    /// Regression: when the panel appears on a tight terminal, the fixed regions below the
    /// chat (active / panel / input / info) must keep exactly the heights they were asked
    /// for — the chat absorbs any overflow instead of squeezing the composer below the
    /// height its cursor needs.
    #[test]
    fn panel_on_tight_terminal_does_not_squeeze_fixed_regions() {
        // A tall multi-line composer (requested 8) on a short 20-row terminal, with panel.
        let cfg = LayoutConfig {
            chat_height: 12,
            active_height: 5,
            panel_height: 3,
            input_height: 8,
            info_height: 1,
            ..LayoutConfig::default()
        };
        let layout = cfg.compute(rect(20));
        assert_eq!(layout.active_area.height, 5);
        assert_eq!(layout.panel_area.height, 3);
        assert_eq!(layout.input_area.height, 8, "composer must not be squeezed");
        assert_eq!(layout.info_area.height, 1);
        // The chat is what gave up the room to accommodate the added panel.
        assert!(layout.chat_area.height < cfg.chat_height);
    }
}
