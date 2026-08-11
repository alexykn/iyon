//! Generic retained Scene host.
//!
//! This module owns frame geometry, component synchronization, focus, ticks,
//! and native History pressure. Application code supplies only semantic Scene
//! state and consumes routed outputs.

use std::time::Instant;

use anyhow::Result;

use crate::{
    Theme,
    backend::NativeHistorySink,
    component::{ComponentRegistry, MountGraph, MountedComponents, TickOutcome, TickScheduler},
    geometry::Size,
    interaction::{
        FocusState, InteractionResult, KeyRouter, KeyStroke, MountedCapabilities, route_paste,
        route_paste_interceptor,
    },
    output::{OutputQueue, OutputRouter},
    physical::Surface,
    presentation::{layout::ViewCompiler, paint::ViewPainter},
};

use super::{
    LayoutSynchronizer, ResolveError, ResolvedRootScene, ResolvedSceneLayout, Scene,
    layout_resolved_scene, resolve_root_scene,
};

const MAX_LAYOUT_PASSES: usize = 8;

/// A fully synchronized frame ready for the terminal adapter.
#[derive(Debug)]
pub(crate) struct PreparedSceneFrame {
    pub(crate) surface: Surface,
    pub(crate) history_overlay: Option<crate::history::HistoryPhysicalOverlay>,
}

/// Generic runtime host for one semantic Scene.
struct StableScene {
    root: ResolvedRootScene,
    layout: ResolvedSceneLayout,
}

pub(crate) struct SceneHost {
    mounted: MountedComponents,
    synchronizer: LayoutSynchronizer,
    focus: FocusState,
    key_router: KeyRouter,
    ticker: TickScheduler,
    outputs: OutputQueue,
    graph: MountGraph,
    capabilities: MountedCapabilities,
}

impl Default for SceneHost {
    fn default() -> Self {
        Self {
            mounted: MountedComponents::default(),
            synchronizer: LayoutSynchronizer::default(),
            focus: FocusState::default(),
            key_router: KeyRouter::new(),
            ticker: TickScheduler::new(),
            outputs: OutputQueue::new(),
            graph: MountGraph::default(),
            capabilities: MountedCapabilities::default(),
        }
    }
}

impl SceneHost {
    pub(crate) fn next_tick_deadline(&self) -> Option<Instant> {
        self.ticker.next_deadline()
    }

    #[cfg(test)]
    pub(crate) fn focused(&self) -> Option<crate::component::ComponentId> {
        self.focus.focused()
    }

    #[cfg(test)]
    pub(crate) fn mount_count_for_test(&self) -> usize {
        self.graph.nodes.len()
    }

    #[cfg(test)]
    pub(crate) fn focusable_count_for_test(&self) -> usize {
        self.capabilities
            .entries
            .values()
            .filter(|capabilities| capabilities.focusable)
            .count()
    }

    pub(crate) fn dispatch_key_local(
        &mut self,
        key: KeyStroke,
        registry: &mut ComponentRegistry,
    ) -> InteractionResult {
        self.key_router.dispatch_local(
            key,
            &mut self.focus,
            &self.graph,
            &self.capabilities,
            registry,
            &mut self.outputs,
        )
    }

    pub(crate) fn intercept_paste<A>(
        &self,
        text: &str,
        intercept: impl FnMut(crate::component::ComponentId, &str) -> Option<A>,
    ) -> Option<A> {
        route_paste_interceptor(text, &self.focus, &self.graph, intercept)
    }

    pub(crate) fn dispatch_paste(
        &mut self,
        text: &str,
        registry: &mut ComponentRegistry,
    ) -> InteractionResult {
        route_paste(
            text,
            &self.focus,
            &self.graph,
            &self.capabilities,
            registry,
            &mut self.outputs,
        )
    }

    pub(crate) fn drain_outputs<A>(
        &mut self,
        router: &OutputRouter<A>,
    ) -> Result<Vec<A>, crate::output::OutputDispatchError> {
        router.drain(&mut self.outputs)
    }

    pub(crate) fn tick_due(
        &mut self,
        now: Instant,
        registry: &mut ComponentRegistry,
    ) -> TickOutcome {
        self.ticker
            .tick_due_with_events(now, registry, &mut self.outputs)
    }

    /// Resolves, synchronizes, paints, and—when necessary—promotes the generic
    /// History prefix. The viewport callback is the only terminal-size seam.
    #[cfg(test)]
    pub(crate) fn render<S, F>(
        &mut self,
        scene: &mut Scene,
        registry: &mut ComponentRegistry,
        theme: &Theme,
        sink: &mut S,
        viewport: F,
    ) -> Result<PreparedSceneFrame, SceneHostError<S::Error>>
    where
        S: NativeHistorySink,
        F: FnMut(&mut S) -> Result<Size>,
    {
        self.render_at(Instant::now(), scene, registry, theme, sink, viewport)
    }

    pub(crate) fn render_at<S, F>(
        &mut self,
        now: Instant,
        scene: &mut Scene,
        registry: &mut ComponentRegistry,
        theme: &Theme,
        sink: &mut S,
        mut viewport: F,
    ) -> Result<PreparedSceneFrame, SceneHostError<S::Error>>
    where
        S: NativeHistorySink,
        F: FnMut(&mut S) -> Result<Size>,
    {
        loop {
            let size = viewport(sink).map_err(SceneHostError::Viewport)?;
            let resolved = self.resolve_stable_at(scene, registry, size, now)?;
            if resolved.root.history_overflow_rows == 0 {
                return Ok(self.paint(resolved, theme));
            }

            let Some(history) = scene.history_mut() else {
                return Ok(self.paint(resolved, theme));
            };
            let transfer = crate::history::transfer_native_prefix_with_theme(
                history,
                sink,
                size.width,
                resolved.root.history_overflow_rows,
                theme,
            )
            .map_err(SceneHostError::Transfer)?;
            match transfer.status {
                crate::history::NativeTransferStatus::Progress => continue,
                crate::history::NativeTransferStatus::Idle
                | crate::history::NativeTransferStatus::SinkBlocked
                | crate::history::NativeTransferStatus::SemanticBlocked { .. } => {
                    return Ok(self.paint(resolved, theme));
                }
            }
        }
    }

    #[cfg(test)]
    fn resolve_stable<E>(
        &mut self,
        scene: &Scene,
        registry: &mut ComponentRegistry,
        size: Size,
    ) -> Result<StableScene, SceneHostError<E>> {
        self.resolve_stable_at(scene, registry, size, Instant::now())
    }

    fn resolve_stable_at<E>(
        &mut self,
        scene: &Scene,
        registry: &mut ComponentRegistry,
        size: Size,
        now: Instant,
    ) -> Result<StableScene, SceneHostError<E>> {
        for _ in 0..MAX_LAYOUT_PASSES {
            let resolved =
                resolve_root_scene(scene, registry, size).map_err(SceneHostError::Resolve)?;
            let layout = layout_resolved_scene(&resolved.scene, size);
            let sync = self.synchronizer.synchronize(
                &resolved.scene.mounts,
                &resolved.scene.capabilities,
                &layout.components,
                registry,
            );
            if matches!(sync, crate::scene::LayoutSync::Dirty) {
                continue;
            }

            self.graph = resolved.scene.mounts.clone();
            self.capabilities = resolved.scene.capabilities.clone();
            let transitions = self.mounted.reconcile(self.graph.clone());
            self.ticker
                .sync_capabilities(&self.graph, &self.capabilities, &transitions, now);
            let focus_changed = self.focus.reconcile_with_geometry(
                &self.graph,
                &self.capabilities,
                Some(&layout.components),
                registry,
            );
            if focus_changed {
                continue;
            }
            return Ok(StableScene {
                root: resolved,
                layout,
            });
        }
        Err(SceneHostError::DidNotConverge)
    }

    fn paint(&self, resolved: StableScene, theme: &Theme) -> PreparedSceneFrame {
        let compiler = ViewCompiler::with_interaction(theme, self.focus.focused(), &self.graph);
        let surface = ViewPainter.paint_tree(&compiler, &resolved.layout.tree);
        PreparedSceneFrame {
            surface,
            history_overlay: resolved.root.history_overlay,
        }
    }
}

#[derive(Debug)]
pub(crate) enum SceneHostError<E> {
    Viewport(anyhow::Error),
    Resolve(ResolveError),
    Transfer(crate::history::NativeTransferError<E>),
    DidNotConverge,
}

impl<E: std::fmt::Debug> std::fmt::Display for SceneHostError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Scene host error: {self:?}")
    }
}

impl<E: std::fmt::Debug + 'static> std::error::Error for SceneHostError<E> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BorderSpec, ColorSpec, Component, ComponentCx, InteractionResult, IntoView, Key, KeyStroke,
        Scene, ScrollPane, StreamingSource, StyleSelector, TextSpan, ThemeColor, View,
        backend::NativeHistorySink,
        component::ComponentRegistry,
        geometry::Size,
        physical::PhysicalRow,
        stream::{
            StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamSnapshotBuilder,
        },
    };

    #[derive(Debug)]
    struct LayoutAware {
        changed: bool,
        calls: usize,
    }

    impl Component for LayoutAware {
        fn view(&self) -> View {
            if self.changed {
                View::text("new\nrow").into_view()
            } else {
                View::text("old").into_view()
            }
        }

        fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
            cx.on_layout_changed(Self::layout_changed);
        }
    }

    impl LayoutAware {
        fn layout_changed(&mut self, _size: Size) {
            self.calls += 1;
            if self.calls == 1 {
                self.changed = true;
            }
        }
    }

    #[derive(Debug, Default)]
    struct EmptySealedSource;

    impl StreamingSource for EmptySealedSource {
        fn snapshot(&self) -> StreamSnapshot {
            StreamSnapshotBuilder::new(
                StreamRevision::new(0),
                StreamOffset::ZERO,
                StreamOffset::ZERO,
                StreamOffset::ZERO,
            )
            .exact_text(
                StreamRange::new(StreamOffset::ZERO, StreamOffset::ZERO),
                [TextSpan::plain("")],
            )
            .finish()
            .unwrap()
        }

        fn seal(&mut self) {}

        fn is_sealed(&self) -> bool {
            true
        }
    }

    #[derive(Default)]
    struct TestSink {
        rows: Vec<PhysicalRow>,
    }

    impl NativeHistorySink for TestSink {
        type Error = ();

        fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
            self.rows.extend(rows.iter().cloned());
            Ok(rows.len())
        }
    }

    #[test]
    fn routes_scroll_pane_locally_and_preserves_detachment_on_content_update() {
        let content = |count: usize| {
            View::text(
                (1..=count)
                    .map(|row| format!("row {row}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        };
        let mut registry = ComponentRegistry::new();
        let pane = registry.register(ScrollPane::new(content(30)));
        let scene = Scene::new(View::component(pane));
        let mut host = SceneHost::default();
        let _ = host
            .resolve_stable::<()>(&scene, &mut registry, Size::new(12, 5))
            .unwrap();

        assert_eq!(
            host.dispatch_key_local(KeyStroke::new(Key::PageUp), &mut registry),
            InteractionResult::Consumed
        );
        assert!(!registry.with(pane, ScrollPane::is_following_end).unwrap());
        registry
            .with_mut(pane, |pane| pane.set_content(content(40)))
            .unwrap();
        assert!(!registry.with(pane, ScrollPane::is_following_end).unwrap());
        assert_eq!(
            host.dispatch_key_local(KeyStroke::new(Key::End), &mut registry),
            InteractionResult::Consumed
        );
        assert!(registry.with(pane, ScrollPane::is_following_end).unwrap());
    }

    struct StatefulField;

    impl Component for StatefulField {
        fn view(&self) -> View {
            View::text("state")
                .foreground(ColorSpec::theme("accent"))
                .into_view()
        }
    }

    struct FocusWithinShell {
        child: crate::component::ComponentHandle<ThemedField>,
    }

    impl Component for FocusWithinShell {
        fn view(&self) -> View {
            View::component(self.child)
                .border(BorderSpec::plain().color(ColorSpec::theme("shell.border")))
                .fill_width()
        }
    }

    struct FocusableShell {
        child: crate::component::ComponentHandle<ThemedField>,
    }

    impl Component for FocusableShell {
        fn view(&self) -> View {
            View::component(self.child)
                .border(BorderSpec::plain().color(ColorSpec::theme("shell.border")))
                .fill_width()
        }

        fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
            cx.focusable();
        }
    }

    struct ThemedField;

    impl Component for ThemedField {
        fn view(&self) -> View {
            View::text("field")
                .border(BorderSpec::plain().color(ColorSpec::theme("field.border")))
                .fill_width()
                .into_view()
        }

        fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
            cx.focusable();
        }
    }

    struct FocusMutatingField {
        focused: bool,
    }

    impl FocusMutatingField {
        fn focus_changed(&mut self, focused: bool) {
            self.focused = focused;
        }
    }

    impl Component for FocusMutatingField {
        fn view(&self) -> View {
            View::text(if self.focused { "focused" } else { "unfocused" }).into_view()
        }

        fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
            cx.focusable();
            cx.on_focus_changed(Self::focus_changed);
        }
    }

    #[test]
    fn semantic_state_crosses_component_boundary_and_nearest_override_wins() {
        let mut registry = ComponentRegistry::new();
        let field = registry.register(StatefulField);
        let scene = Scene::new(
            View::component(field)
                .style_state("severity", "warning")
                .into_view(),
        );
        let theme = Theme::new()
            .with_color("accent", ThemeColor::Indexed(2))
            .with_color_variant(
                "accent",
                StyleSelector::state("severity", "warning"),
                ThemeColor::Indexed(1),
            )
            .with_color_variant(
                "accent",
                StyleSelector::state("severity", "error"),
                ThemeColor::Indexed(3),
            );
        let mut host = SceneHost::default();
        let stable = host
            .resolve_stable::<()>(&scene, &mut registry, Size::new(20, 4))
            .unwrap();
        let frame = host.paint(stable, &theme);
        assert_eq!(
            frame.surface.get(0, 0).style.foreground,
            Some(crate::physical::PhysicalColor::Indexed(1))
        );

        let nested = Scene::new(
            View::vertical(|column| {
                column.child(
                    View::component(field)
                        .style_state("severity", "error")
                        .into_view(),
                );
            })
            .style_state("severity", "warning"),
        );
        let stable = host
            .resolve_stable::<()>(&nested, &mut registry, Size::new(20, 4))
            .unwrap();
        let frame = host.paint(stable, &theme);
        assert_eq!(
            frame.surface.get(0, 0).style.foreground,
            Some(crate::physical::PhysicalColor::Indexed(3))
        );
    }

    #[test]
    fn focus_callback_mutation_converges_before_paint() {
        let mut registry = ComponentRegistry::new();
        let field = registry.register(FocusMutatingField { focused: false });
        let scene = Scene::new(View::component(field));
        let mut host = SceneHost::default();
        let stable = host
            .resolve_stable::<()>(&scene, &mut registry, Size::new(20, 4))
            .unwrap();
        let frame = host.paint(stable, &Theme::default());

        assert_eq!(frame.surface.get(0, 0).grapheme.as_deref(), Some("f"));
        assert_eq!(registry.with(field, |field| field.focused), Some(true));
    }

    #[test]
    fn paints_framework_focus_variant_without_component_revision_change() {
        let mut registry = ComponentRegistry::new();
        let field = registry.register(ThemedField);
        let scene = Scene::new(View::component(field));
        let theme = Theme::new()
            .with_color("field.border", ThemeColor::Named(crate::AnsiColor::Gray))
            .with_color_variant(
                "field.border",
                StyleSelector::focused(),
                ThemeColor::Named(crate::AnsiColor::Cyan),
            );
        let revision = registry.revision(field).expect("field revision");
        let mut host = SceneHost::default();
        let stable = host
            .resolve_stable::<()>(&scene, &mut registry, Size::new(20, 4))
            .unwrap();
        let frame = host.paint(stable, &theme);

        assert_eq!(host.focused(), Some(field.id()));
        assert_eq!(registry.revision(field), Some(revision));
        assert_eq!(
            frame.surface.get(0, 0).style.foreground,
            Some(crate::physical::PhysicalColor::Named(
                crate::physical::AnsiColor::Cyan,
            ))
        );
    }

    #[test]
    fn paints_focus_within_on_a_component_parent_without_leaking_focus() {
        let mut registry = ComponentRegistry::new();
        let child = registry.register(ThemedField);
        let shell = registry.register(FocusWithinShell { child });
        let scene = Scene::new(View::component(shell));
        let theme = Theme::new()
            .with_color("shell.border", ThemeColor::Named(crate::AnsiColor::Gray))
            .with_color_variant(
                "shell.border",
                StyleSelector::focus_within(),
                ThemeColor::Named(crate::AnsiColor::Cyan),
            );
        let mut host = SceneHost::default();
        let stable = host
            .resolve_stable::<()>(&scene, &mut registry, Size::new(20, 4))
            .unwrap();
        let frame = host.paint(stable, &theme);

        assert_eq!(host.focused(), Some(child.id()));
        assert_eq!(
            frame.surface.get(0, 0).style.foreground,
            Some(crate::physical::PhysicalColor::Named(
                crate::physical::AnsiColor::Cyan,
            ))
        );
    }

    #[test]
    fn parent_focused_does_not_mark_nested_child_focused() {
        let mut registry = ComponentRegistry::new();
        let child = registry.register(ThemedField);
        let shell = registry.register(FocusableShell { child });
        let scene = Scene::new(View::component(shell));
        let theme = Theme::new()
            .with_color("shell.border", ThemeColor::Named(crate::AnsiColor::Gray))
            .with_color("field.border", ThemeColor::Named(crate::AnsiColor::Gray))
            .with_color_variant(
                "field.border",
                StyleSelector::focused(),
                ThemeColor::Named(crate::AnsiColor::Cyan),
            );
        let mut host = SceneHost::default();
        let stable = host
            .resolve_stable::<()>(&scene, &mut registry, Size::new(20, 5))
            .unwrap();
        let frame = host.paint(stable, &theme);

        assert_eq!(host.focused(), Some(shell.id()));
        assert_eq!(
            frame.surface.get(0, 1).style.foreground,
            Some(crate::physical::PhysicalColor::Named(
                crate::physical::AnsiColor::Gray,
            ))
        );
    }

    #[test]
    fn paints_the_layout_from_the_stable_convergence_pass() {
        let mut registry = ComponentRegistry::new();
        let handle = registry.register(LayoutAware {
            changed: false,
            calls: 0,
        });
        let scene = Scene::new(View::component(handle));
        let mut host = SceneHost::default();

        let stable = host
            .resolve_stable::<()>(&scene, &mut registry, Size::new(10, 4))
            .unwrap();
        let geometry = stable
            .layout
            .components
            .entries
            .get(&handle.id())
            .expect("layout-aware component geometry")
            .content;
        let frame = host.paint(stable, &crate::Theme::default());

        assert_eq!(registry.with(handle, |component| component.calls), Some(2));
        assert_eq!(geometry.height, 2);
        assert_eq!(frame.surface.get(0, 0).grapheme.as_deref(), Some("n"));
        assert_eq!(frame.surface.get(0, 1).grapheme.as_deref(), Some("r"));
    }

    #[test]
    fn render_continues_after_zero_row_stream_retirement() {
        let mut history = crate::History::new();
        let stream = history.push_stream(EmptySealedSource).unwrap();
        history.seal_stream(stream).unwrap();
        history.push("S1\nS2\nS3").unwrap();
        let mut scene = Scene::with_history(history, "body");
        let mut registry = ComponentRegistry::new();
        let mut host = SceneHost::default();
        let mut sink = TestSink::default();

        host.render(
            &mut scene,
            &mut registry,
            &crate::Theme::default(),
            &mut sink,
            |_| Ok(Size::new(10, 3)),
        )
        .unwrap();

        assert!(sink.rows.iter().any(|row| row.plain_text() == "S1"));
    }
}
