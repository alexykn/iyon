//! Bottom-bar panels: a single-at-a-time, content-sized region just above the
//! composer that hosts passive widgets (the steering queue) and, later, interactive
//! ones (slash menu / file picker).
//!
//! Panels are the presentation layer for a strip of UI; they own their state, report a
//! content-derived height (0 collapses the region entirely), render into whatever area
//! the layout grants (clipping themselves when tight), and may optionally consume keys.
//! They emit neutral `PanelAction` values — never the TUI's `FrontendAction` — keeping
//! this module decoupled from the controller.

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Stable identity for a panel, used to route messages and set which panel is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelKind {
    SteeringQueue,
}

/// State-update messages pushed into panels. Fanning these out lets each panel consume
/// only what it owns, so new panels don't require touching the dispatch logic.
#[derive(Debug, Clone)]
pub(crate) enum PanelMessage {
    SteeringQueued(String),
    SteeringDelivered(String),
}

/// Neutral actions a panel can emit. Empty for now: the steering queue is passive and
/// never produces actions. Interactive panels (slash menu) will add variants such as
/// `RunCommand(String)` / `Close`.
#[derive(Debug)]
pub(crate) enum PanelAction {}

pub(crate) trait Panel: std::fmt::Debug {
    fn kind(&self) -> PanelKind;
    /// Content-derived height; `0` collapses the panel's region entirely.
    fn desired_height(&self, width: u16) -> u16;
    fn render(&self, frame: &mut Frame, area: Rect);
    fn handle_message(&mut self, _msg: &PanelMessage) {}
    /// Returns `Some` only when this panel is focused/interactive and consumes the key.
    fn handle_key(&mut self, _key: KeyEvent) -> Option<Vec<PanelAction>> {
        None
    }
    fn focus(&mut self) {}
    fn unfocus(&mut self) {}
}

const MAX_PENDING_ROWS: usize = 4;

/// Passive panel that lists steering messages queued for the agent but not yet sent.
#[derive(Debug)]
pub(crate) struct SteeringQueuePanel {
    pending: Vec<String>,
}

impl SteeringQueuePanel {
    pub(crate) fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }
}

impl Panel for SteeringQueuePanel {
    fn kind(&self) -> PanelKind {
        PanelKind::SteeringQueue
    }

    fn desired_height(&self, _width: u16) -> u16 {
        if self.pending.is_empty() {
            return 0;
        }
        let shown = self.pending.len().min(MAX_PENDING_ROWS);
        let overflow = usize::from(self.pending.len() > MAX_PENDING_ROWS);
        (shown + overflow) as u16
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        if self.pending.is_empty() || area.height == 0 {
            return;
        }
        let style = Style::default()
            .fg(Color::Rgb(128, 128, 128))
            .add_modifier(Modifier::DIM);

        let mut lines: Vec<Line<'static>> = self
            .pending
            .iter()
            .take(MAX_PENDING_ROWS)
            .map(|text| Line::from(Span::styled(format!("Steering: {text}"), style)))
            .collect();
        let overflow = self.pending.len().saturating_sub(MAX_PENDING_ROWS);
        if overflow > 0 {
            lines.push(Line::from(Span::styled(
                format!("↑ {overflow} more queued"),
                style,
            )));
        }

        // Defensive clip: only paint as many rows as were actually granted.
        lines.truncate(usize::from(area.height));
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn handle_message(&mut self, msg: &PanelMessage) {
        match msg {
            PanelMessage::SteeringQueued(text) => self.pending.push(text.clone()),
            PanelMessage::SteeringDelivered(text) => {
                if let Some(pos) = self.pending.iter().position(|t| t == text) {
                    self.pending.remove(pos);
                }
            }
        }
    }
}

/// Owns the registry of panels and decides which single panel is visible at a time.
/// Hidden panels keep their state so a transient panel (e.g. slash menu) can be shown
/// over the queue without discarding it; only the visible panel renders / gets keys.
#[derive(Debug)]
pub(crate) struct BottomPanelBar {
    panels: Vec<Box<dyn Panel>>,
    visible: Option<PanelKind>,
}

impl Default for BottomPanelBar {
    fn default() -> Self {
        let mut bar = Self::new();
        bar.register(Box::new(SteeringQueuePanel::new()));
        bar.show(PanelKind::SteeringQueue);
        bar
    }
}

impl BottomPanelBar {
    pub(crate) fn new() -> Self {
        Self {
            panels: Vec::new(),
            visible: None,
        }
    }

    pub(crate) fn register(&mut self, panel: Box<dyn Panel>) {
        if !self.panels.iter().any(|p| p.kind() == panel.kind()) {
            self.panels.push(panel);
        }
    }

    pub(crate) fn show(&mut self, kind: PanelKind) {
        if !self.panels.iter().any(|p| p.kind() == kind) {
            return;
        }
        if let Some(p) = self
            .visible
            .filter(|prev| *prev != kind)
            .and_then(|prev| self.panels.iter_mut().find(|p| p.kind() == prev))
        {
            p.as_mut().unfocus();
        }
        self.visible = Some(kind);
        if let Some(p) = self.panels.iter_mut().find(|p| p.kind() == kind) {
            p.as_mut().focus();
        }
    }

    /// Hide the bar (returns to a collapsed region). Used by future interactive panels
    /// (e.g. slash menu) when they close; currently referenced only by tests.
    #[allow(dead_code)]
    pub(crate) fn hide(&mut self) {
        if let Some(p) = self
            .visible
            .and_then(|kind| self.panels.iter_mut().find(|p| p.kind() == kind))
        {
            p.as_mut().unfocus();
        }
        self.visible = None;
    }

    /// Which panel is currently visible, if any.
    #[allow(dead_code)]
    pub(crate) fn visible(&self) -> Option<PanelKind> {
        self.visible
    }

    /// Fan a state message out to every registered panel (each consumes what it owns).
    pub(crate) fn send(&mut self, msg: &PanelMessage) {
        for panel in self.panels.iter_mut() {
            panel.handle_message(msg);
        }
    }

    fn active(&self) -> Option<&(dyn Panel + '_)> {
        let kind = self.visible?;
        let slot = self.panels.iter().find(|p| p.kind() == kind)?;
        Some(slot.as_ref())
    }

    fn active_mut(&mut self) -> Option<&mut (dyn Panel + '_)> {
        let kind = self.visible?;
        let slot = self.panels.iter_mut().find(|p| p.kind() == kind)?;
        Some(slot.as_mut())
    }

    pub(crate) fn desired_height(&self, width: u16) -> u16 {
        self.active().map_or(0, |p| p.desired_height(width))
    }

    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        if let Some(panel) = self.active() {
            panel.render(frame, area);
        }
    }

    /// Route a key to the visible panel if it is interactive and consumes it.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Option<Vec<PanelAction>> {
        self.active_mut()?.handle_key(key)
    }

    pub(crate) fn steer_queued(&mut self, text: String) {
        self.send(&PanelMessage::SteeringQueued(text));
    }

    pub(crate) fn steer_delivered(&mut self, text: &str) {
        self.send(&PanelMessage::SteeringDelivered(text.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steering_queue_empty_height_is_zero() {
        let panel = SteeringQueuePanel::new();
        assert_eq!(panel.desired_height(80), 0);
    }

    #[test]
    fn steering_queue_queued_then_delivered() {
        let mut panel = SteeringQueuePanel::new();
        panel.handle_message(&PanelMessage::SteeringQueued("first".to_string()));
        panel.handle_message(&PanelMessage::SteeringQueued("second".to_string()));
        assert_eq!(panel.desired_height(80), 2);

        panel.handle_message(&PanelMessage::SteeringDelivered("first".to_string()));
        assert_eq!(panel.pending, vec!["second"]);
        panel.handle_message(&PanelMessage::SteeringDelivered("second".to_string()));
        assert_eq!(panel.desired_height(80), 0);
    }

    #[test]
    fn bottom_panel_bar_collapses_when_visible_empty() {
        let bar = BottomPanelBar::default();
        assert_eq!(bar.desired_height(80), 0);
        assert_eq!(bar.visible(), Some(PanelKind::SteeringQueue));
    }

    #[test]
    fn bottom_panel_bar_keeps_hidden_state_through_show_hide() {
        let mut bar = BottomPanelBar::default();
        bar.steer_queued("pending".to_string());
        assert_eq!(bar.desired_height(80), 1);

        // Hiding the bar collapses the region, but the panel's state is preserved and
        // reappears when shown again.
        bar.hide();
        assert_eq!(bar.desired_height(80), 0);
        bar.show(PanelKind::SteeringQueue);
        assert_eq!(bar.desired_height(80), 1);
        assert_eq!(bar.visible(), Some(PanelKind::SteeringQueue));
    }

    #[test]
    fn passive_panel_returns_no_keys_so_composer_handles_them() {
        // The visible steering queue is passive: it never consumes keys, so the bar
        // routes them on to the composer untouched.
        let mut bar = BottomPanelBar::default();
        assert!(
            bar.handle_key(KeyEvent::new(
                crossterm::event::KeyCode::Char('x'),
                crossterm::event::KeyModifiers::NONE
            ))
            .is_none()
        );
    }
}
