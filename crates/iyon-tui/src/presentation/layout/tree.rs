use std::collections::{HashMap, HashSet};

use crate::{
    component::ComponentId,
    geometry::{Rect, Size},
    presentation::api::style::{StyleFacts, StyleStates},
    presentation::ir::{Decoration, TextView, ViewId},
    presentation::{OverflowIndicator, WidthRule},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LayoutNodeId(pub(crate) usize);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutStyle {
    pub(crate) component_scope: Option<ComponentId>,
    pub(crate) style_states: StyleStates,
    pub(crate) style_facts: StyleFacts,
    pub(crate) decoration: Decoration,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayoutContent {
    Text {
        text: TextView,
        width_rule: WidthRule,
    },
    Spacer {
        rows: u16,
    },
    Children,
    Clamp {
        overflow: OverflowIndicator,
    },
    RowViewport {
        skip_rows: u16,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutNode {
    pub(crate) view_id: ViewId,
    pub(crate) paint_cacheable: bool,
    pub(crate) rect: Rect,
    pub(crate) content_rect: Rect,
    pub(crate) clip_rect: Rect,
    pub(crate) component: Option<ComponentId>,
    pub(crate) children: Vec<LayoutNodeId>,
    pub(crate) style: LayoutStyle,
    pub(crate) content: LayoutContent,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SignedRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl SignedRect {
    fn from(rect: Rect) -> Self {
        Self {
            x: i32::from(rect.x),
            y: i32::from(rect.y),
            width: i32::from(rect.width),
            height: i32::from(rect.height),
        }
    }

    fn translate_y(self, offset: i32) -> Self {
        Self {
            y: self.y.saturating_add(offset),
            ..self
        }
    }

    fn right(self) -> i32 {
        self.x.saturating_add(self.width)
    }

    fn bottom(self) -> i32 {
        self.y.saturating_add(self.height)
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (left < right && top < bottom).then_some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    fn to_rect(self) -> Rect {
        Rect::new(
            self.x.clamp(0, i32::from(u16::MAX)) as u16,
            self.y.clamp(0, i32::from(u16::MAX)) as u16,
            self.width.clamp(0, i32::from(u16::MAX)) as u16,
            self.height.clamp(0, i32::from(u16::MAX)) as u16,
        )
    }

    fn to_rect_opt(self) -> Option<Rect> {
        (self.width > 0 && self.height > 0).then(|| self.to_rect())
    }
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
        let root = SignedRect::from(Rect::new(0, 0, self.size.width, self.size.height));
        self.collect_component_geometry(self.root, 0, root, &mut entries);
        ComponentGeometryMap { entries }
    }

    fn collect_component_geometry(
        &self,
        id: LayoutNodeId,
        offset_y: i32,
        inherited_clip: SignedRect,
        entries: &mut HashMap<ComponentId, ComponentGeometry>,
    ) {
        let node = self.node(id);
        let rect = SignedRect::from(node.rect).translate_y(offset_y);
        let content = SignedRect::from(node.content_rect).translate_y(offset_y);
        let clip = SignedRect::from(node.clip_rect)
            .translate_y(offset_y)
            .intersection(inherited_clip)
            .unwrap_or(SignedRect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
        if let Some(component) = node.component {
            entries.insert(
                component,
                ComponentGeometry {
                    outer: rect.to_rect(),
                    content: content.to_rect(),
                    visible: rect.intersection(clip).and_then(|rect| rect.to_rect_opt()),
                },
            );
        }

        let child_offset = match node.content {
            LayoutContent::RowViewport { skip_rows } => {
                offset_y.saturating_sub(i32::from(skip_rows))
            }
            _ => offset_y,
        };
        for child in &node.children {
            self.collect_component_geometry(*child, child_offset, clip, entries);
        }
    }

    pub(crate) fn validate(&self) -> bool {
        if self.nodes.get(self.root.0).is_none() {
            return false;
        }
        let root_rect = Rect::new(0, 0, self.size.width, self.size.height);
        let mut parents = vec![None; self.nodes.len()];
        for (parent, node) in self.nodes.iter().enumerate() {
            for child in &node.children {
                if child.0 < parents.len() {
                    parents[child.0] = Some(LayoutNodeId(parent));
                }
            }
        }
        if !self.nodes.iter().enumerate().all(|(index, node)| {
            node.children.iter().all(|child| child.0 < self.nodes.len())
                && contains(node.rect, node.content_rect)
                && (contains(root_rect, node.clip_rect)
                    || self.has_viewport_ancestor(LayoutNodeId(index), &parents))
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

    fn has_viewport_ancestor(&self, id: LayoutNodeId, parents: &[Option<LayoutNodeId>]) -> bool {
        let mut current = parents[id.0];
        while let Some(parent) = current {
            if matches!(self.node(parent).content, LayoutContent::RowViewport { .. }) {
                return true;
            }
            current = parents[parent.0];
        }
        false
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
