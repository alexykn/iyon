//! Stateful docked presentation outside durable conversation history.
//!
//! FEATURE EXTENSION API.
//!
//! Panels own feature state and describe their appearance with generic semantic
//! views. The dock host owns registration, visibility, focus, and physical
//! sizing; it does not know what a panel's content means.

use std::{any::Any, marker::PhantomData};

use crate::presentation::{DockPanel, DockSizePolicy, InteractionResult, IntoView, UiKey, View};

/// FEATURE EXTENSION API. Opaque registration handle for a dock panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelId(u64);

/// FEATURE EXTENSION API. Typed registration handle for owning-feature access.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PanelHandle<P> {
    id: PanelId,
    marker: PhantomData<fn() -> P>,
}

impl<P> Copy for PanelHandle<P> {}

impl<P> Clone for PanelHandle<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> PanelHandle<P> {
    pub(crate) fn id(self) -> PanelId {
        self.id
    }

    fn new(id: PanelId) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }
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

    pub(crate) fn queued(&mut self, text: String) {
        self.pending.push(text);
    }

    pub(crate) fn delivered(&mut self, text: &str) {
        if let Some(pos) = self.pending.iter().position(|item| item == text) {
            self.pending.remove(pos);
        }
    }
}

impl DockPanel for SteeringQueuePanel {
    fn view(&self) -> View {
        if self.pending.is_empty() {
            return View::spacer(0);
        }

        let mut children = self
            .pending
            .iter()
            .take(MAX_PENDING_ROWS)
            .map(|text| {
                View::text(format!("Steering: {text}"))
                    .width(crate::presentation::WidthRule::Fill)
                    .into_view()
            })
            .collect::<Vec<_>>();
        let overflow = self.pending.len().saturating_sub(MAX_PENDING_ROWS);
        if overflow > 0 {
            children.push(
                View::text(format!("↑ {overflow} more queued"))
                    .width(crate::presentation::WidthRule::Fill)
                    .into_view(),
            );
        }
        View::column(children, 0).width(crate::presentation::WidthRule::Fill)
    }

    fn size_policy(&self) -> DockSizePolicy {
        DockSizePolicy::HiddenWhenEmpty
    }

    fn handle_key(&mut self, _key: UiKey) -> InteractionResult {
        InteractionResult::Ignored
    }
}

struct PanelSlot {
    id: PanelId,
    panel: Box<dyn DockPanel>,
}

impl std::fmt::Debug for PanelSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanelSlot").field("id", &self.id).finish()
    }
}

/// FEATURE EXTENSION API. Registry and lifecycle host for bottom-anchored UI.
#[derive(Debug)]
pub(crate) struct BottomPanelBar {
    panels: Vec<PanelSlot>,
    visible: Option<PanelId>,
    next_id: u64,
}

impl Default for BottomPanelBar {
    fn default() -> Self {
        Self::new()
    }
}

impl BottomPanelBar {
    pub(crate) fn new() -> Self {
        Self {
            panels: Vec::new(),
            visible: None,
            next_id: 1,
        }
    }

    /// FEATURE EXTENSION API. Register a stateful panel and receive its handle.
    pub(crate) fn register<P>(&mut self, panel: P) -> PanelHandle<P>
    where
        P: DockPanel + 'static,
    {
        let id = PanelId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.panels.push(PanelSlot {
            id,
            panel: Box::new(panel),
        });
        PanelHandle::new(id)
    }

    /// FEATURE EXTENSION API. Access a panel through the owning feature's typed handle.
    pub(crate) fn with_mut<P, R>(
        &mut self,
        handle: PanelHandle<P>,
        f: impl FnOnce(&mut P) -> R,
    ) -> Option<R>
    where
        P: DockPanel + 'static,
    {
        let slot = self.panels.iter_mut().find(|slot| slot.id == handle.id)?;
        let panel = slot.panel.as_any_mut().downcast_mut::<P>()?;
        Some(f(panel))
    }

    pub(crate) fn show<P>(&mut self, handle: PanelHandle<P>) {
        let id = handle.id();
        if !self.panels.iter().any(|slot| slot.id == id) {
            return;
        }
        if let Some(previous) = self.visible.filter(|previous| *previous != id) {
            if let Some(slot) = self.panels.iter_mut().find(|slot| slot.id == previous) {
                slot.panel.blur();
            }
        }
        self.visible = Some(id);
        if let Some(slot) = self.panels.iter_mut().find(|slot| slot.id == id) {
            slot.panel.focus();
        }
    }

    /// Hide the bar while retaining every panel's state.
    pub(crate) fn hide(&mut self) {
        if let Some(id) = self.visible.take()
            && let Some(slot) = self.panels.iter_mut().find(|slot| slot.id == id)
        {
            slot.panel.blur();
        }
    }

    #[cfg(test)]
    pub(crate) fn visible(&self) -> Option<PanelId> {
        self.visible
    }

    pub(crate) fn desired_height(&self, width: u16) -> u16 {
        let Some(panel) = self.active() else {
            return 0;
        };
        let measured = crate::presentation::internal::view_height(&panel.view(), width);
        match panel.size_policy() {
            DockSizePolicy::HiddenWhenEmpty => measured,
            DockSizePolicy::Content { max_rows } => {
                max_rows.map_or(measured, |max| measured.min(max))
            }
            DockSizePolicy::Fixed(height) => height,
        }
    }

    pub(crate) fn view(&self) -> Option<View> {
        self.active().map(DockPanel::view)
    }

    pub(crate) fn handle_key(&mut self, key: UiKey) -> InteractionResult {
        self.active_mut()
            .map_or(InteractionResult::Ignored, |panel| panel.handle_key(key))
    }

    fn active(&self) -> Option<&dyn DockPanel> {
        let id = self.visible?;
        self.panels
            .iter()
            .find(|slot| slot.id == id)
            .map(|slot| slot.panel.as_ref())
    }

    fn active_mut(&mut self) -> Option<&mut dyn DockPanel> {
        let id = self.visible?;
        self.panels
            .iter_mut()
            .find(|slot| slot.id == id)
            .map(|slot| slot.panel.as_mut())
    }
}

impl dyn DockPanel {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn steering_queue_empty_height_is_zero() {
        let panel = SteeringQueuePanel::new();
        assert_eq!(panel.size_policy(), DockSizePolicy::HiddenWhenEmpty);
        assert!(matches!(
            panel.view().kind,
            crate::presentation::ir::ViewKind::Spacer { rows: 0 }
        ));
    }

    #[test]
    fn steering_queue_queued_then_delivered() {
        let mut panel = SteeringQueuePanel::new();
        panel.queued("first".to_string());
        panel.queued("second".to_string());
        assert_eq!(
            crate::presentation::internal::view_height(&panel.view(), 80),
            2
        );
        panel.delivered("first");
        panel.delivered("second");
        assert!(matches!(
            panel.view().kind,
            crate::presentation::ir::ViewKind::Spacer { rows: 0 }
        ));
    }

    #[test]
    fn bottom_panel_bar_collapses_when_visible_empty() {
        let bar = BottomPanelBar::default();
        assert_eq!(bar.desired_height(80), 0);
        assert!(bar.visible().is_none());
    }

    #[test]
    fn bottom_panel_bar_keeps_hidden_state_through_show_hide() {
        let mut bar = BottomPanelBar::default();
        let steering = bar.register(SteeringQueuePanel::new());
        bar.show(steering);
        let _ = bar.with_mut(steering, |panel| panel.queued("pending".to_string()));
        assert_eq!(bar.desired_height(80), 1);
        bar.hide();
        assert_eq!(bar.desired_height(80), 0);
        let id = bar.visible();
        assert!(id.is_none());
        bar.show(steering);
        assert_eq!(bar.desired_height(80), 1);
    }

    #[test]
    fn passive_panel_returns_no_keys_so_composer_handles_them() {
        let mut bar = BottomPanelBar::default();
        assert_eq!(bar.handle_key(UiKey::Char('x')), InteractionResult::Ignored);
        let _ = (KeyCode::Char('x'), KeyModifiers::NONE);
    }
}
