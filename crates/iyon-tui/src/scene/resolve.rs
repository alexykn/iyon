use std::fmt;

use crate::{
    component::{ComponentId, ComponentRegistry, MountGraph, MountNode},
    interaction::MountedCapabilities,
    presentation::{
        View,
        ir::{ClampRowsView, ColumnChild, ColumnView, ContainerNode, RowChild, RowView, ViewKind},
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

pub(crate) fn resolve_scene(
    view: &View,
    registry: &ComponentRegistry,
) -> Result<ResolvedScene, ResolveError> {
    let mut resolver = Resolver {
        registry,
        mounts: Vec::new(),
        capabilities: MountedCapabilities::default(),
        seen: Vec::new(),
        active: Vec::new(),
    };
    let view = resolver.resolve_view(view, None)?;
    Ok(ResolvedScene {
        view,
        mounts: MountGraph::new(resolver.mounts),
        capabilities: resolver.capabilities,
    })
}

struct Resolver<'a> {
    registry: &'a ComponentRegistry,
    mounts: Vec<MountNode>,
    capabilities: MountedCapabilities,
    seen: Vec<ComponentId>,
    active: Vec<ComponentId>,
}

impl Resolver<'_> {
    fn resolve_view(
        &mut self,
        view: &View,
        parent: Option<ComponentId>,
    ) -> Result<View, ResolveError> {
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
            ViewKind::Container(container) => ViewKind::Container(ContainerNode {
                child: Box::new(self.resolve_view(&container.child, parent)?),
            }),
            ViewKind::ClampRows(clamp) => ViewKind::ClampRows(ClampRowsView {
                child: Box::new(self.resolve_view(&clamp.child, parent)?),
                max_rows: clamp.max_rows,
                overflow: clamp.overflow.clone(),
            }),
            ViewKind::ComponentSlot(slot) => return self.resolve_slot(view, slot.id, parent),
        };

        let mut resolved = view.clone();
        resolved.kind = kind;
        Ok(resolved)
    }

    fn resolve_slot(
        &mut self,
        slot: &View,
        id: ComponentId,
        parent: Option<ComponentId>,
    ) -> Result<View, ResolveError> {
        let Some((component_view, revision)) = self.registry.view_for_resolution(id) else {
            return Err(ResolveError::MissingComponent { id });
        };
        if let Some(position) = self.active.iter().position(|active| *active == id) {
            let mut path = self.active[position..].to_vec();
            path.push(id);
            return Err(ResolveError::ComponentCycle { path });
        }
        if self.seen.contains(&id) {
            return Err(ResolveError::DuplicateComponent { id });
        }

        self.seen.push(id);
        self.mounts.push(MountNode {
            id,
            parent,
            revision,
        });
        self.capabilities.insert(
            id,
            self.registry
                .capabilities(id)
                .expect("registered component capabilities disappeared during resolution"),
        );
        self.active.push(id);
        let resolved = self.resolve_view(&component_view, Some(id));
        self.active.pop();
        let resolved = resolved?;

        Ok(View {
            component: Some(id),
            width: slot.width,
            height: slot.height,
            decoration: slot.decoration.clone(),
            kind: ViewKind::Container(ContainerNode {
                child: Box::new(resolved),
            }),
        })
    }
}
