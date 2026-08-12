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

    pub(crate) fn parent(&self, id: ComponentId) -> Option<ComponentId> {
        self.nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.parent)
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.nodes.iter().map(|node| node.id)
    }

    pub(crate) fn is_descendant_or_self(&self, id: ComponentId, ancestor: ComponentId) -> bool {
        let mut current = Some(id);
        while let Some(candidate) = current {
            if candidate == ancestor {
                return true;
            }
            current = self.parent(candidate);
        }
        false
    }
}
