use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone)]
pub(crate) struct LayoutConfig {
    pub(crate) spacer_height: u16,
    pub(crate) conversation_height: u16,
    pub(crate) conversation_gap_height: u16,
    pub(crate) panel_height: u16,
    pub(crate) input_height: u16,
    pub(crate) info_height: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            spacer_height: 0,
            conversation_height: 0,
            conversation_gap_height: 0,
            panel_height: 0,
            input_height: 3,
            info_height: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ComputedLayout {
    pub(crate) spacer_area: Rect,
    pub(crate) conversation_area: Rect,
    pub(crate) conversation_gap_area: Rect,
    pub(crate) panel_area: Rect,
    pub(crate) input_area: Rect,
    pub(crate) info_area: Rect,
    pub(crate) conversation_capacity_rows: usize,
}

impl LayoutConfig {
    pub(crate) fn compute(&self, area: Rect) -> ComputedLayout {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(self.spacer_height),
                // Conversation is the only flexible visual surface. Fixed regions
                // below it retain their requested heights and conversation absorbs
                // pressure when the terminal is tight.
                Constraint::Max(self.conversation_height),
                Constraint::Length(self.conversation_gap_height),
                Constraint::Length(self.panel_height),
                Constraint::Length(self.input_height),
                Constraint::Length(self.info_height),
            ])
            .split(area);

        let conversation_capacity_rows = usize::from(vertical[1].height);
        ComputedLayout {
            spacer_area: vertical[0],
            conversation_area: vertical[1],
            conversation_gap_area: vertical[2],
            panel_area: vertical[3],
            input_area: vertical[4],
            info_area: vertical[5],
            conversation_capacity_rows,
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
            conversation_height: 10,
            conversation_gap_height: 1,
            panel_height: 3,
            input_height: 3,
            info_height: 1,
            ..LayoutConfig::default()
        };
        let layout = cfg.compute(rect(30));
        assert_eq!(layout.panel_area.height, 3);
        assert!(layout.conversation_gap_area.bottom() <= layout.panel_area.y);
        assert!(layout.panel_area.bottom() <= layout.input_area.y);
    }

    /// Regression: when the panel appears on a tight terminal, the fixed regions below the
    /// conversation (panel / input / info) must keep exactly the heights they were asked for —
    /// the conversation absorbs any overflow instead of squeezing the composer below the height
    /// its cursor needs.
    #[test]
    fn panel_on_tight_terminal_does_not_squeeze_fixed_regions() {
        // A tall multi-line composer (requested 8) on a short 20-row terminal, with panel.
        let cfg = LayoutConfig {
            conversation_height: 12,
            conversation_gap_height: 1,
            panel_height: 3,
            input_height: 8,
            info_height: 1,
            ..LayoutConfig::default()
        };
        let layout = cfg.compute(rect(20));
        assert_eq!(layout.panel_area.height, 3);
        assert_eq!(layout.input_area.height, 8, "composer must not be squeezed");
        assert_eq!(layout.info_area.height, 1);
        // The conversation is what gave up the room to accommodate the added panel.
        assert!(layout.conversation_area.height < cfg.conversation_height);
    }
}
