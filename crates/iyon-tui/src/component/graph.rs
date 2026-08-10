use super::{ComponentId, ComponentRevision};

/// Semantic component placements in depth-first mount order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MountGraph {
    pub(crate) nodes: Vec<MountNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MountNode {
    pub(crate) id: ComponentId,
    pub(crate) parent: Option<ComponentId>,
    pub(crate) revision: ComponentRevision,
}

impl MountGraph {
    pub(crate) fn new(nodes: Vec<MountNode>) -> Self {
        Self { nodes }
    }

    pub(crate) fn contains(&self, id: ComponentId) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.nodes.iter().map(|node| node.id)
    }
}
