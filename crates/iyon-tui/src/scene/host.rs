//! Generic retained Scene host.
//!
//! This module owns frame geometry, component synchronization, focus, ticks,
//! and native History pressure. Application code supplies only semantic Scene
//! state and consumes routed outputs.

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::{
    backend::NativeHistorySink,
    component::{ComponentRegistry, MountGraph, MountedComponents, TickScheduler},
    geometry::Size,
    interaction::{
        FocusState, InteractionResult, KeyRouter, KeyStroke, MountedCapabilities, route_paste,
    },
    output::{OutputQueue, OutputRouter},
    physical::Surface,
    presentation::{
        layout::{LayoutEngine, ManualLayoutEngine, ViewCompiler},
        paint::ViewPainter,
    },
};

use super::{LayoutSynchronizer, ResolveError, ResolvedRootScene, Scene, resolve_root_scene};

const MAX_LAYOUT_PASSES: usize = 8;

/// A fully synchronized frame ready for the terminal adapter.
#[derive(Debug)]
pub(crate) struct PreparedSceneFrame {
    pub(crate) surface: Surface,
    pub(crate) history_overlay: Option<crate::history::HistoryPhysicalOverlay>,
    pub(crate) physically_complete: bool,
}

/// Generic runtime host for one semantic Scene.
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

    pub(crate) fn tick_due(&mut self, now: Instant, registry: &mut ComponentRegistry) -> bool {
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
            if resolved.history_overflow_rows == 0 {
                return Ok(self.paint(resolved, size));
            }

            let Some(history) = scene.history_mut() else {
                return Ok(self.paint(resolved, size));
            };
            let transfer = crate::history::transfer_native_prefix(
                history,
                sink,
                size.width,
                resolved.history_overflow_rows,
            )
            .map_err(SceneHostError::Transfer)?;
            if transfer.inserted == 0 {
                return Ok(self.paint(resolved, size));
            }
        }
    }

    fn resolve_stable<E>(
        &mut self,
        scene: &Scene,
        registry: &mut ComponentRegistry,
        size: Size,
    ) -> Result<ResolvedRootScene, SceneHostError<E>> {
        for _ in 0..MAX_LAYOUT_PASSES {
            let resolved =
                resolve_root_scene(scene, registry, size).map_err(SceneHostError::Resolve)?;
            let layout = crate::scene::layout_resolved_scene(&resolved.scene, size);
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
            return Ok(resolved);
        }
        Err(SceneHostError::DidNotConverge)
    }

    fn paint(&self, resolved: ResolvedRootScene, size: Size) -> PreparedSceneFrame {
        let compiler = ViewCompiler::default();
        let tree = ManualLayoutEngine.layout(
            &resolved.scene.view,
            crate::geometry::LayoutConstraints::bounded(size),
        );
        let surface = ViewPainter.paint_tree(&compiler, &tree);
        let complete = surface.physically_complete;
        PreparedSceneFrame {
            surface,
            history_overlay: resolved.history_overlay,
            physically_complete: complete,
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
