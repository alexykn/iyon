use std::collections::{HashMap, HashSet};

use crate::{
    component::ComponentId,
    geometry::{Rect, Size},
    presentation::{OverflowIndicator, View},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LayoutNodeId(pub(crate) usize);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayoutPayload {
    View(View),
    Clamp {
        max_rows: u16,
        overflow: OverflowIndicator,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutNode {
    pub(crate) rect: Rect,
    pub(crate) content_rect: Rect,
    pub(crate) clip_rect: Rect,
    pub(crate) component: Option<ComponentId>,
    pub(crate) children: Vec<LayoutNodeId>,
    pub(crate) view: View,
    pub(crate) payload: LayoutPayload,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutTree {
    pub(crate) root: LayoutNodeId,
    pub(crate) nodes: Vec<LayoutNode>,
    pub(crate) size: Size,
    pub(crate) physically_complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ComponentGeometry {
    pub(crate) outer: Rect,
    pub(crate) content: Rect,
    pub(crate) visible: Option<Rect>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComponentGeometryMap {
    pub(crate) entries: HashMap<ComponentId, ComponentGeometry>,
}

fn contains(outer: Rect, inner: Rect) -> bool {
    if inner.is_empty() {
        return inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x <= outer.right()
            && inner.y <= outer.bottom();
    }
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

impl LayoutTree {
    pub(crate) fn node(&self, id: LayoutNodeId) -> &LayoutNode {
        &self.nodes[id.0]
    }

    pub(crate) fn component_geometry(&self) -> ComponentGeometryMap {
        let mut entries = HashMap::new();
        for node in &self.nodes {
            let Some(id) = node.component else {
                continue;
            };
            entries.insert(
                id,
                ComponentGeometry {
                    outer: node.rect,
                    content: node.content_rect,
                    visible: node.rect.intersection(node.clip_rect),
                },
            );
        }
        ComponentGeometryMap { entries }
    }

    pub(crate) fn validate(&self) -> bool {
        if self.nodes.get(self.root.0).is_none() {
            return false;
        }
        let root_rect = Rect::new(0, 0, self.size.width, self.size.height);
        if !self.nodes.iter().all(|node| {
            node.children.iter().all(|child| child.0 < self.nodes.len())
                && contains(node.rect, node.content_rect)
                && contains(root_rect, node.clip_rect)
        }) {
            return false;
        }
        let mut component_ids = HashSet::new();
        if !self
            .nodes
            .iter()
            .filter_map(|node| node.component)
            .all(|id| component_ids.insert(id))
        {
            return false;
        }
        let mut states = vec![0u8; self.nodes.len()];
        if !self.visit(self.root, &mut states) {
            return false;
        }
        states.into_iter().all(|state| state == 2)
    }

    fn visit(&self, id: LayoutNodeId, states: &mut [u8]) -> bool {
        if states[id.0] == 1 {
            return false;
        }
        if states[id.0] == 2 {
            return true;
        }
        states[id.0] = 1;
        for child in &self.nodes[id.0].children {
            if !self.visit(*child, states) {
                return false;
            }
        }
        states[id.0] = 2;
        true
    }
}
