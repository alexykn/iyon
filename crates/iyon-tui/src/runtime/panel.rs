//! Stateful docked presentation outside durable conversation history.
//!
//! The bar owns dock placement and capability dispatch. Component instances
//! themselves live in the shared [`ComponentRegistry`].

use crate::component::{Component, ComponentHandle, ComponentId, ComponentRegistry};
use crate::presentation::{DockPanel, DockSizePolicy, InteractionResult, IntoView, UiKey, View};

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

impl Component for SteeringQueuePanel {
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
                    .fill_width()
                    .into_view()
            })
            .collect::<Vec<_>>();
        let overflow = self.pending.len().saturating_sub(MAX_PENDING_ROWS);
        if overflow > 0 {
            children.push(
                View::text(format!("↑ {overflow} more queued"))
                    .fill_width()
                    .into_view(),
            );
        }
        View::column(children, 0).fill_width()
    }
}

impl DockPanel for SteeringQueuePanel {
    fn size_policy(&self) -> DockSizePolicy {
        DockSizePolicy::HiddenWhenEmpty
    }

    fn handle_key(&mut self, _key: UiKey) -> InteractionResult {
        InteractionResult::Ignored
    }
}

#[derive(Clone, Copy)]
struct PanelOps {
    size_policy: fn(&ComponentRegistry, ComponentId) -> Option<DockSizePolicy>,
    handle_key: fn(&mut ComponentRegistry, ComponentId, UiKey) -> InteractionResult,
    focus: fn(&mut ComponentRegistry, ComponentId),
    blur: fn(&mut ComponentRegistry, ComponentId),
}

impl std::fmt::Debug for PanelOps {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PanelOps(..)")
    }
}

impl PanelOps {
    fn for_type<P>() -> Self
    where
        P: DockPanel,
    {
        Self {
            size_policy: size_policy::<P>,
            handle_key: handle_key::<P>,
            focus: focus::<P>,
            blur: blur::<P>,
        }
    }
}

fn size_policy<P>(registry: &ComponentRegistry, id: ComponentId) -> Option<DockSizePolicy>
where
    P: DockPanel,
{
    registry.with(ComponentHandle::<P>::from_id(id), DockPanel::size_policy)
}

fn handle_key<P>(registry: &mut ComponentRegistry, id: ComponentId, key: UiKey) -> InteractionResult
where
    P: DockPanel,
{
    registry
        .with_mut(ComponentHandle::<P>::from_id(id), |panel| {
            panel.handle_key(key)
        })
        .unwrap_or(InteractionResult::Ignored)
}

fn focus<P>(registry: &mut ComponentRegistry, id: ComponentId)
where
    P: DockPanel,
{
    let _ = registry.with_mut(ComponentHandle::<P>::from_id(id), DockPanel::focus);
}

fn blur<P>(registry: &mut ComponentRegistry, id: ComponentId)
where
    P: DockPanel,
{
    let _ = registry.with_mut(ComponentHandle::<P>::from_id(id), DockPanel::blur);
}

struct PanelSlot {
    id: ComponentId,
    ops: PanelOps,
}

impl std::fmt::Debug for PanelSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PanelSlot")
            .field("id", &self.id)
            .finish()
    }
}

/// Registry and lifecycle host for bottom-anchored UI.
#[derive(Debug, Default)]
pub(crate) struct BottomPanelBar {
    panels: Vec<PanelSlot>,
    visible: Option<ComponentId>,
}

impl BottomPanelBar {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Register a stateful panel in the shared component registry.
    pub(crate) fn register<P>(
        &mut self,
        components: &mut ComponentRegistry,
        panel: P,
    ) -> ComponentHandle<P>
    where
        P: DockPanel,
    {
        let handle = components.register(panel);
        self.panels.push(PanelSlot {
            id: handle.id(),
            ops: PanelOps::for_type::<P>(),
        });
        handle
    }

    pub(crate) fn show<P>(&mut self, components: &mut ComponentRegistry, handle: ComponentHandle<P>)
    where
        P: DockPanel,
    {
        let id = handle.id();
        if !self.contains(id) || !components.contains(handle) {
            return;
        }
        if let Some(previous) = self.visible.filter(|previous| *previous != id) {
            self.dispatch(previous, |ops| (ops.blur)(components, previous));
        }
        self.visible = Some(id);
        self.dispatch(id, |ops| (ops.focus)(components, id));
    }

    /// Hide the bar while retaining every registered component's state.
    pub(crate) fn hide(&mut self, components: &mut ComponentRegistry) {
        let Some(id) = self.visible.take() else {
            return;
        };
        self.dispatch(id, |ops| (ops.blur)(components, id));
    }

    #[cfg(test)]
    pub(crate) fn visible(&self) -> Option<ComponentId> {
        self.visible
    }

    pub(crate) fn desired_height(&self, components: &ComponentRegistry, width: u16) -> u16 {
        let Some(id) = self.visible else {
            return 0;
        };
        let Some(view) = components.render_registered(id) else {
            return 0;
        };
        let measured = crate::presentation::layout::view_height(&view, width);
        match self
            .panel_ops(id)
            .and_then(|ops| (ops.size_policy)(components, id))
        {
            Some(DockSizePolicy::HiddenWhenEmpty) => measured,
            Some(DockSizePolicy::Content { max_rows }) => {
                max_rows.map_or(measured, |max| measured.min(max))
            }
            Some(DockSizePolicy::Fixed(height)) => height,
            None => 0,
        }
    }

    pub(crate) fn view(&self, components: &ComponentRegistry) -> Option<View> {
        self.visible.and_then(|id| components.render_registered(id))
    }

    pub(crate) fn handle_key(
        &self,
        components: &mut ComponentRegistry,
        key: UiKey,
    ) -> InteractionResult {
        let Some(id) = self.visible else {
            return InteractionResult::Ignored;
        };
        self.panel_ops(id)
            .map_or(InteractionResult::Ignored, |ops| {
                (ops.handle_key)(components, id, key)
            })
    }

    fn contains(&self, id: ComponentId) -> bool {
        self.panels.iter().any(|panel| panel.id == id)
    }

    fn panel_ops(&self, id: ComponentId) -> Option<PanelOps> {
        self.panels
            .iter()
            .find(|panel| panel.id == id)
            .map(|panel| panel.ops)
    }

    fn dispatch(&self, id: ComponentId, operation: impl FnOnce(PanelOps)) {
        if let Some(ops) = self.panel_ops(id) {
            operation(ops);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ComponentRegistry;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[derive(Debug)]
    struct FocusPanel {
        focused: bool,
    }

    impl Component for FocusPanel {
        fn view(&self) -> View {
            View::text("focus").into_view()
        }
    }

    impl DockPanel for FocusPanel {
        fn size_policy(&self) -> DockSizePolicy {
            DockSizePolicy::Fixed(1)
        }

        fn handle_key(&mut self, _key: UiKey) -> InteractionResult {
            if self.focused {
                InteractionResult::Consumed
            } else {
                InteractionResult::Ignored
            }
        }

        fn focus(&mut self) {
            self.focused = true;
        }

        fn blur(&mut self) {
            self.focused = false;
        }
    }

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
            crate::presentation::layout::view_height(&panel.view(), 80),
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
        let components = ComponentRegistry::new();
        let bar = BottomPanelBar::default();
        assert_eq!(bar.desired_height(&components, 80), 0);
        assert!(bar.visible().is_none());
    }

    #[test]
    fn bottom_panel_bar_keeps_hidden_state_through_show_hide() {
        let mut components = ComponentRegistry::new();
        let mut bar = BottomPanelBar::default();
        let steering = bar.register(&mut components, SteeringQueuePanel::new());
        bar.show(&mut components, steering);
        let _ = components.with_mut(steering, |panel| panel.queued("pending".to_string()));
        assert_eq!(bar.desired_height(&components, 80), 1);
        bar.hide(&mut components);
        assert_eq!(bar.desired_height(&components, 80), 0);
        assert!(bar.visible().is_none());
        bar.show(&mut components, steering);
        assert_eq!(bar.desired_height(&components, 80), 1);
    }

    #[test]
    fn passive_panel_returns_no_keys_so_composer_handles_them() {
        let mut components = ComponentRegistry::new();
        let mut bar = BottomPanelBar::default();
        let steering = bar.register(&mut components, SteeringQueuePanel::new());
        bar.show(&mut components, steering);
        assert_eq!(
            bar.handle_key(&mut components, UiKey::Char('x')),
            InteractionResult::Ignored
        );
        let _ = (KeyCode::Char('x'), KeyModifiers::NONE);
    }

    #[test]
    fn panel_focus_blur_and_key_dispatch_remain_unchanged() {
        let mut components = ComponentRegistry::new();
        let mut bar = BottomPanelBar::default();
        let panel = bar.register(&mut components, FocusPanel { focused: false });

        assert_eq!(
            bar.handle_key(&mut components, UiKey::Char('x')),
            InteractionResult::Ignored
        );
        bar.show(&mut components, panel);
        assert_eq!(
            bar.handle_key(&mut components, UiKey::Char('x')),
            InteractionResult::Consumed
        );
        bar.hide(&mut components);
        assert_eq!(
            bar.handle_key(&mut components, UiKey::Char('x')),
            InteractionResult::Ignored
        );
        bar.show(&mut components, panel);
        assert_eq!(
            bar.handle_key(&mut components, UiKey::Char('x')),
            InteractionResult::Consumed
        );
    }

    #[test]
    fn removing_panel_state_leaves_dock_without_a_state_object() {
        let mut components = ComponentRegistry::new();
        let mut bar = BottomPanelBar::default();
        let steering = bar.register(&mut components, SteeringQueuePanel::new());
        bar.show(&mut components, steering);
        assert!(components.remove(steering).is_some());
        assert_eq!(bar.desired_height(&components, 80), 0);
        assert!(bar.view(&components).is_none());
        assert_eq!(
            bar.handle_key(&mut components, UiKey::Char('x')),
            InteractionResult::Ignored
        );
    }
}
