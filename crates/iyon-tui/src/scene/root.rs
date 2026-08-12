//! Public semantic terminal-root composition.

use crate::{History, IntoView, View};

/// The semantic root of a terminal application.
///
/// A [`Scene`] contains an optional root-level [`History`] followed by one
/// ordinary [`View`] body. It is intentionally not an [`IntoView`] value:
/// roots cannot be nested inside ordinary presentation composition.
///
/// ```text
/// let scene = Scene::new(View::text("ordinary application"));
///
/// let mut history = History::new();
/// history.push("earlier output")?;
/// let scene = Scene::with_history(history, View::text("body"));
/// ```
pub struct Scene {
    history: Option<History>,
    body: View,
}

impl Scene {
    /// Creates a body-only semantic root.
    pub fn new(body: impl IntoView) -> Self {
        Self {
            history: None,
            body: body.into_view(),
        }
    }

    /// Creates a semantic root with one root-level History and an ordinary
    /// body below it.
    pub fn with_history(history: History, body: impl IntoView) -> Self {
        Self {
            history: Some(history),
            body: body.into_view(),
        }
    }

    /// Returns the optional root-level History.
    pub fn history(&self) -> Option<&History> {
        self.history.as_ref()
    }

    /// Returns mutable access to the optional root-level History.
    pub fn history_mut(&mut self) -> Option<&mut History> {
        self.history.as_mut()
    }

    /// Returns the ordinary body View.
    pub fn body(&self) -> &View {
        &self.body
    }

    /// Replaces the ordinary body View.
    pub fn set_body(&mut self, body: impl IntoView) {
        self.body = body.into_view();
    }
}

use crate::{
    component::ComponentRegistry,
    geometry::Size,
    history::{HistoryPhysicalOverlay, project_into_session_for_host},
    presentation::{
        ir::{ColumnChild, ColumnView, HeightRule, TrackSize, ViewKind, WidthRule},
        layout::measure_view,
    },
};

use super::{ResolveError, ResolveSession, ResolvedScene};

/// Private resolved root state used by the host/layout pipeline.
#[derive(Debug, PartialEq)]
pub(crate) struct ResolvedRootScene {
    pub(crate) scene: ResolvedScene,
    pub(crate) history_overlay: Option<HistoryPhysicalOverlay>,
    pub(crate) history_overflow_rows: usize,
    pub(crate) history_height: u16,
    pub(crate) body_height: u16,
}

/// Resolves a semantic root with one component-resolution domain.
///
/// The first resolution is a read-only body measurement prepass. The real
/// session then resolves History first and Body second, preserving visual and
/// mount order across the root boundary.
pub(crate) fn resolve_root_scene(
    root: &Scene,
    registry: &ComponentRegistry,
    size: Size,
) -> Result<ResolvedRootScene, ResolveError> {
    let body_height = body_height(root.body(), registry, size.width, size.height)?;
    let history_height = root
        .history
        .as_ref()
        .map_or(0, |_| size.height.saturating_sub(body_height));

    let mut session = ResolveSession::new(registry);
    let (history_view, history_overlay, history_overflow_rows) = match root.history.as_ref() {
        Some(history) => {
            let projection = project_into_session_for_host(
                history,
                Size::new(size.width, history_height),
                &mut session,
            )?;
            (
                projection.view,
                projection.frozen_overlay,
                projection.overflow_rows,
            )
        }
        None => (View::spacer(0), None, 0),
    };
    let body_view = session
        .resolve_root(root.body())?
        .fill_width()
        .fill_height();
    let root_view = root_view(root.history.is_some().then_some(history_view), body_view);
    let scene = session.finish(root_view);

    Ok(ResolvedRootScene {
        scene,
        history_overlay,
        history_overflow_rows,
        history_height,
        body_height,
    })
}

fn body_height(
    body: &View,
    registry: &ComponentRegistry,
    width: u16,
    terminal_height: u16,
) -> Result<u16, ResolveError> {
    let mut session = ResolveSession::new(registry);
    let resolved = session.resolve_root(body)?;
    Ok(measure_view(&resolved, width).height.min(terminal_height))
}

fn root_view(history: Option<View>, body: View) -> View {
    let mut children = Vec::with_capacity(usize::from(history.is_some()) + 1);
    if let Some(history) = history {
        children.push(ColumnChild {
            track: TrackSize::Flex { min: 0 },
            view: history,
        });
    }
    children.push(ColumnChild {
        track: TrackSize::Content { max: None },
        view: body,
    });
    View {
        component: None,
        width: WidthRule::Fill,
        height: HeightRule::Fill,
        decoration: Default::default(),
        style_states: Vec::new(),
        component_scope: None,
        kind: ViewKind::Column(ColumnView { children, gap: 0 }),
    }
}
