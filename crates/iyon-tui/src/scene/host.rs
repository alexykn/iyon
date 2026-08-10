//! Generic retained Scene host.
//!
//! This module owns frame geometry, component synchronization, focus, ticks,
//! and native History pressure. Application code supplies only semantic Scene
//! state and consumes routed outputs.

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::{
    backend::NativeHistorySink,
    component::{ComponentRegistry, MountGraph, MountedComponents, TickOutcome, TickScheduler},
    geometry::Size,
    interaction::{
        FocusState, InteractionResult, KeyRouter, KeyStroke, MountedCapabilities, route_paste,
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
    pub(crate) fn next_wait_timeout(&self, now: Instant, idle: Duration) -> Duration {
        self.ticker.next_timeout(now, idle)
    }

    pub(crate) fn focused(&self) -> Option<crate::component::ComponentId> {
        self.focus.focused()
    }

    pub(crate) fn dispatch_key(
        &mut self,
        key: KeyStroke,
        registry: &mut ComponentRegistry,
    ) -> InteractionResult {
        self.key_router.dispatch(
            key,
            &mut self.focus,
            &self.graph,
            &self.capabilities,
            registry,
            &mut self.outputs,
        )
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
    pub(crate) fn render<S, F>(
        &mut self,
        scene: &mut Scene,
        registry: &mut ComponentRegistry,
        sink: &mut S,
        mut viewport: F,
    ) -> Result<PreparedSceneFrame, SceneHostError<S::Error>>
    where
        S: NativeHistorySink,
        F: FnMut(&mut S) -> Result<Size>,
    {
        loop {
            let size = viewport(sink).map_err(SceneHostError::Viewport)?;
            let resolved = self.resolve_stable(scene, registry, size)?;
            if resolved.root.history_overflow_rows == 0 {
                return Ok(self.paint(resolved));
            }

            let Some(history) = scene.history_mut() else {
                return Ok(self.paint(resolved));
            };
            let transfer = crate::history::transfer_native_prefix(
                history,
                sink,
                size.width,
                resolved.root.history_overflow_rows,
            )
            .map_err(SceneHostError::Transfer)?;
            match transfer.status {
                crate::history::NativeTransferStatus::Progress => continue,
                crate::history::NativeTransferStatus::Idle
                | crate::history::NativeTransferStatus::SinkBlocked
                | crate::history::NativeTransferStatus::SemanticBlocked { .. } => {
                    return Ok(self.paint(resolved));
                }
            }
        }
    }

    fn resolve_stable<E>(
        &mut self,
        scene: &Scene,
        registry: &mut ComponentRegistry,
        size: Size,
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
            self.ticker.sync_capabilities(
                &self.graph,
                &self.capabilities,
                &transitions,
                Instant::now(),
            );
            self.focus.reconcile_with_geometry(
                &self.graph,
                &self.capabilities,
                Some(&layout.components),
                registry,
            );
            return Ok(StableScene {
                root: resolved,
                layout,
            });
        }
        Err(SceneHostError::DidNotConverge)
    }

    fn paint(&self, resolved: StableScene) -> PreparedSceneFrame {
        let compiler = ViewCompiler::default();
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
        Component, ComponentCx, IntoView, Scene, StreamingSource, TextSpan, View,
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
        let frame = host.paint(stable);

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

        host.render(&mut scene, &mut registry, &mut sink, |_| {
            Ok(Size::new(10, 3))
        })
        .unwrap();

        assert!(sink.rows.iter().any(|row| row.plain_text() == "S1"));
    }
}
