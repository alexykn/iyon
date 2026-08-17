use std::{collections::HashSet, fmt};

use crate::{
    component::{ComponentId, ComponentRegistry, MountGraph, MountNode},
    interaction::MountedCapabilities,
    perf::{self, Counter},
    presentation::{
        View,
        ir::{
            ClampRowsView, ColumnChild, ColumnView, ContainerNode, HangingView, RowChild, RowView,
            ViewKind,
        },
    },
};

use super::ResolvedScene;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResolveError {
    MissingComponent { id: ComponentId },
    DuplicateComponent { id: ComponentId },
    ComponentCycle { path: Vec<ComponentId> },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingComponent { id } => write!(formatter, "missing component {id:?}"),
            Self::DuplicateComponent { id } => write!(formatter, "duplicate component {id:?}"),
            Self::ComponentCycle { path } => write!(formatter, "component cycle {path:?}"),
        }
    }
}

impl std::error::Error for ResolveError {}

#[cfg(test)]
pub(crate) fn resolve_scene(
    view: &View,
    registry: &ComponentRegistry,
) -> Result<ResolvedScene, ResolveError> {
    let mut session = ResolveSession::new(registry);
    let view = session.resolve_root(view)?;
    Ok(session.finish(view))
}

pub(crate) struct ResolveSession<'a> {
    resolver: Resolver<'a>,
}

impl<'a> ResolveSession<'a> {
    pub(crate) fn new(registry: &'a ComponentRegistry) -> Self {
        Self {
            resolver: Resolver {
                registry,
                mounts: Vec::new(),
                capabilities: MountedCapabilities::default(),
                seen: HashSet::new(),
                active: Vec::new(),
            },
        }
    }

    pub(crate) fn resolve_root(&mut self, view: &View) -> Result<View, ResolveError> {
        self.resolver.resolve_view(view, None)
    }

    pub(crate) fn finish(self, view: View) -> ResolvedScene {
        ResolvedScene {
            view,
            mounts: MountGraph::new(self.resolver.mounts),
            capabilities: self.resolver.capabilities,
        }
    }
}

struct Resolver<'a> {
    registry: &'a ComponentRegistry,
    mounts: Vec<MountNode>,
    capabilities: MountedCapabilities,
    seen: HashSet<ComponentId>,
    active: Vec<ComponentId>,
}

impl Resolver<'_> {
    fn resolve_view(
        &mut self,
        view: &View,
        parent: Option<ComponentId>,
    ) -> Result<View, ResolveError> {
        perf::inc(Counter::ResolverNodesVisited);
        let kind = match &view.kind {
            ViewKind::Text(text) => ViewKind::Text(text.clone()),
            ViewKind::Spacer { rows } => ViewKind::Spacer { rows: *rows },
            ViewKind::Column(column) => ViewKind::Column(ColumnView {
                children: column
                    .children
                    .iter()
                    .map(|child| {
                        Ok(ColumnChild {
                            track: child.track,
                            view: self.resolve_view(&child.view, parent)?,
                        })
                    })
                    .collect::<Result<_, ResolveError>>()?,
                gap: column.gap,
            }),
            ViewKind::Row(row) => ViewKind::Row(RowView {
                children: row
                    .children
                    .iter()
                    .map(|child| {
                        Ok(RowChild {
                            track: child.track,
                            view: self.resolve_view(&child.view, parent)?,
                        })
                    })
                    .collect::<Result<_, ResolveError>>()?,
                gap: row.gap,
                vertical_align: row.vertical_align,
            }),
            ViewKind::Grid(grid) => ViewKind::Grid(crate::presentation::ir::GridView {
                columns: grid.columns.clone(),
                rows: grid.rows.clone(),
                column_gap: grid.column_gap,
                row_gap: grid.row_gap,
                cells: grid
                    .cells
                    .iter()
                    .map(|cell| {
                        Ok(crate::presentation::ir::GridCellView {
                            row: cell.row,
                            column: cell.column,
                            row_span: cell.row_span,
                            column_span: cell.column_span,
                            horizontal_align: cell.horizontal_align,
                            vertical_align: cell.vertical_align,
                            view: self.resolve_view(&cell.view, parent)?,
                        })
                    })
                    .collect::<Result<_, ResolveError>>()?,
            }),
            ViewKind::Hanging(hanging) => ViewKind::Hanging(HangingView {
                prefix: Box::new(self.resolve_view(&hanging.prefix, parent)?),
                continuation_prefix: Box::new(
                    self.resolve_view(&hanging.continuation_prefix, parent)?,
                ),
                body: Box::new(self.resolve_view(&hanging.body, parent)?),
            }),
            ViewKind::Container(container) => ViewKind::Container(ContainerNode {
                child: Box::new(self.resolve_view(&container.child, parent)?),
            }),
            ViewKind::ClampRows(clamp) => ViewKind::ClampRows(ClampRowsView {
                child: Box::new(self.resolve_view(&clamp.child, parent)?),
                max_rows: clamp.max_rows,
                overflow: clamp.overflow.clone(),
            }),
            ViewKind::RowViewport(viewport) => {
                ViewKind::RowViewport(crate::presentation::ir::RowViewportView {
                    child: Box::new(self.resolve_view(&viewport.child, parent)?),
                    skip_rows: viewport.skip_rows,
                    visible_height: viewport.visible_height,
                    layout_height: viewport.layout_height,
                    intrinsic_content_height: viewport.intrinsic_content_height,
                })
            }
            ViewKind::ComponentSlot(slot) => return self.resolve_slot(view, slot.id, parent),
        };

        Ok(view.clone_shell_with(kind, parent))
    }

    fn resolve_slot(
        &mut self,
        slot: &View,
        id: ComponentId,
        parent: Option<ComponentId>,
    ) -> Result<View, ResolveError> {
        let Some(snapshot) = self.registry.resolution(id) else {
            return Err(ResolveError::MissingComponent { id });
        };
        if let Some(position) = self.active.iter().position(|active| *active == id) {
            let mut path = self.active[position..].to_vec();
            path.push(id);
            return Err(ResolveError::ComponentCycle { path });
        }
        if !self.seen.insert(id) {
            return Err(ResolveError::DuplicateComponent { id });
        }
        self.mounts.push(MountNode {
            id,
            parent,
            revision: snapshot.revision,
        });
        self.capabilities.insert(id, snapshot.capabilities);
        self.active.push(id);
        let resolved = self.resolve_view(&snapshot.view, Some(id));
        self.active.pop();
        let resolved = resolved?;

        perf::inc(Counter::ViewNodesConstructedRust);
        Ok(View {
            component: Some(id),
            width: slot.width,
            height: slot.height,
            decoration: slot.decoration.clone(),
            style_states: slot.style_states.clone(),
            style_facts: slot.style_facts.clone(),
            component_scope: Some(id),
            kind: ViewKind::Container(ContainerNode {
                child: Box::new(resolved),
            }),
        })
    }
}
