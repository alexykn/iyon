use std::{collections::HashSet, fmt};

use crate::{
    component::{ComponentId, ComponentRegistry, MountGraph, MountNode},
    interaction::MountedCapabilities,
    perf::{self, Counter},
    presentation::{View, ir::ViewKind},
};

use super::{ResolutionOverlay, ResolvedScene};

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
                overlay: ResolutionOverlay::default(),
                #[cfg(test)]
                nodes_visited: 0,
                seen: HashSet::new(),
                active: Vec::new(),
            },
        }
    }

    pub(crate) fn resolve_root(&mut self, view: &View) -> Result<View, ResolveError> {
        self.resolver.scan_view(view, None)?;
        Ok(view.clone())
    }

    pub(crate) fn overlay(&self) -> &ResolutionOverlay {
        &self.resolver.overlay
    }

    #[cfg(test)]
    pub(crate) fn nodes_visited(&self) -> usize {
        self.resolver.nodes_visited
    }

    pub(crate) fn finish(self, view: View) -> ResolvedScene {
        ResolvedScene {
            view,
            mounts: MountGraph::new(self.resolver.mounts),
            capabilities: self.resolver.capabilities,
            overlay: self.resolver.overlay,
        }
    }
}

struct Resolver<'a> {
    registry: &'a ComponentRegistry,
    mounts: Vec<MountNode>,
    capabilities: MountedCapabilities,
    overlay: ResolutionOverlay,
    #[cfg(test)]
    nodes_visited: usize,
    seen: HashSet<ComponentId>,
    active: Vec<ComponentId>,
}

impl Resolver<'_> {
    /// Scans only semantic nodes that can contain a component slot. Ordinary
    /// component-free branches are represented by their cached ViewFlags and
    /// require no recursive topology walk.
    fn scan_view(&mut self, view: &View, parent: Option<ComponentId>) -> Result<(), ResolveError> {
        perf::inc(Counter::ResolverNodesVisited);
        #[cfg(test)]
        {
            self.nodes_visited += 1;
        }
        if !view.contains_component_identity() {
            return Ok(());
        }

        match view.kind() {
            ViewKind::Text(_) | ViewKind::Spacer { .. } => Ok(()),
            ViewKind::ComponentSlot(slot) => self.resolve_slot(slot.id, parent),
            ViewKind::Container(container) => self.scan_view(&container.child, parent),
            ViewKind::ClampRows(clamp) => self.scan_view(&clamp.child, parent),
            ViewKind::RowViewport(viewport) => self.scan_view(&viewport.child, parent),
            ViewKind::Hanging(hanging) => {
                self.scan_view(&hanging.prefix, parent)?;
                self.scan_view(&hanging.continuation_prefix, parent)?;
                self.scan_view(&hanging.body, parent)
            }
            ViewKind::Column(column) => {
                for child in column.children.iter() {
                    self.scan_view(&child.view, parent)?;
                }
                Ok(())
            }
            ViewKind::Row(row) => {
                for child in row.children.iter() {
                    self.scan_view(&child.view, parent)?;
                }
                Ok(())
            }
            ViewKind::Grid(grid) => {
                for cell in grid.cells.iter() {
                    self.scan_view(&cell.view, parent)?;
                }
                Ok(())
            }
        }
    }

    fn resolve_slot(
        &mut self,
        id: ComponentId,
        parent: Option<ComponentId>,
    ) -> Result<(), ResolveError> {
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
        self.capabilities.insert(id, snapshot.capabilities.clone());
        self.overlay.components.insert(id, snapshot.clone());
        self.active.push(id);
        let result = self.scan_view(&snapshot.view, Some(id));
        self.active.pop();
        result
    }
}
