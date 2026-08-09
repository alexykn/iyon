use crate::{
    geometry::{AxisConstraint, LayoutConstraints, Rect, Size},
    presentation::ir::{ColumnView, ContainerNode, HeightRule, RowView, View, ViewKind, WidthRule},
};

use super::{
    measure::{horizontal_decoration, intrinsic_content_size, intrinsic_size},
    tracks::allocate_tracks,
    tree::{LayoutNode, LayoutNodeId, LayoutPayload, LayoutTree},
};

pub(crate) trait LayoutEngine {
    fn layout(&self, view: &View, constraints: LayoutConstraints) -> LayoutTree;
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ManualLayoutEngine;

impl LayoutEngine for ManualLayoutEngine {
    fn layout(&self, view: &View, constraints: LayoutConstraints) -> LayoutTree {
        let width = constraints
            .width
            .definite()
            .unwrap_or_else(|| intrinsic_size(view, u16::MAX).width);
        let height = constraints.height.definite();
        let clip = Rect::new(0, 0, width, height.unwrap_or(u16::MAX));
        let mut builder = Builder {
            nodes: Vec::new(),
            physically_complete: true,
        };
        let (root, size, complete) = builder.build(view, 0, 0, Some(width), height, clip);
        builder.physically_complete = complete;
        let size = Size::new(size.width, size.height);
        let mut tree = LayoutTree {
            root,
            nodes: builder.nodes,
            size,
            physically_complete: builder.physically_complete,
        };
        // A width-only layout has no synthetic infinite height in its public result.
        if matches!(constraints.height, AxisConstraint::Unbounded) {
            tree.size.height = tree.node(root).rect.height;
        }
        debug_assert!(tree.validate(), "invalid layout tree: {tree:?}");
        tree
    }
}

struct Builder {
    nodes: Vec<LayoutNode>,
    physically_complete: bool,
}

impl Builder {
    fn build(
        &mut self,
        view: &View,
        x: u16,
        y: u16,
        width_bound: Option<u16>,
        height_bound: Option<u16>,
        clip: Rect,
    ) -> (LayoutNodeId, Size, bool) {
        let border = u16::from(view.decoration.border.is_some());
        let (left_padding, _right_padding, horizontal_decoration) =
            horizontal_decoration(view, width_bound.unwrap_or(u16::MAX));
        let vertical_decoration = border
            .saturating_mul(2)
            .saturating_add(view.decoration.padding.top)
            .saturating_add(view.decoration.padding.bottom);
        let inner_width_bound =
            width_bound.map(|value| value.saturating_sub(horizontal_decoration));
        let intrinsic = intrinsic_size(view, width_bound.unwrap_or(u16::MAX));
        let intrinsic_core = Size::new(
            intrinsic.width.saturating_sub(horizontal_decoration),
            intrinsic.height.saturating_sub(vertical_decoration),
        );
        let core_width = match (view.width, inner_width_bound) {
            (WidthRule::Fill, Some(width)) => width,
            (_, Some(width)) => intrinsic_core.width.min(width),
            (_, None) => intrinsic_core.width,
        };
        let core_height_bound =
            height_bound.map(|height| height.saturating_sub(vertical_decoration));
        let requested_core_height = match (view.height, core_height_bound) {
            (HeightRule::Fill, Some(height)) => height,
            (_, Some(height)) => intrinsic_core.height.min(height),
            (_, None) => intrinsic_core.height,
        };
        let content_x = x.saturating_add(border).saturating_add(left_padding);
        let content_y = y
            .saturating_add(border)
            .saturating_add(view.decoration.padding.top);
        let (children, core_size, complete) = self.build_kind(
            &view.kind,
            content_x,
            content_y,
            core_width,
            requested_core_height,
            view.height == HeightRule::Fill,
            clip,
        );
        let core_width = match view.width {
            WidthRule::Fill => core_width,
            WidthRule::Fit => core_size.width.min(core_width),
        };
        let core_height = match view.height {
            HeightRule::Fill => requested_core_height,
            HeightRule::Fit => core_size.height.min(requested_core_height),
        };
        let size = Size::new(
            core_width
                .saturating_add(horizontal_decoration)
                .min(width_bound.unwrap_or(u16::MAX)),
            core_height
                .saturating_add(vertical_decoration)
                .min(height_bound.unwrap_or(u16::MAX)),
        );
        let rect = Rect::new(x, y, size.width, size.height);
        let node_clip = clip
            .intersection(rect)
            .unwrap_or(Rect::new(clip.x, clip.y, 0, 0));
        let content_rect = Rect::new(content_x, content_y, core_width, core_height)
            .intersection(rect)
            .unwrap_or(Rect::new(rect.x, rect.y, 0, 0));
        let id = LayoutNodeId(self.nodes.len());
        self.nodes.push(LayoutNode {
            rect,
            content_rect,
            clip_rect: node_clip,
            component: view.component,
            children: children.clone(),
            view: view.clone(),
            payload: match &view.kind {
                ViewKind::ClampRows(clamp) => LayoutPayload::Clamp {
                    max_rows: clamp.max_rows,
                    overflow: clamp.overflow.clone(),
                },
                _ => LayoutPayload::View(view.clone()),
            },
        });
        for child in children {
            self.clip_subtree(child, node_clip);
        }
        (id, size, complete)
    }

    fn clip_subtree(&mut self, id: LayoutNodeId, clip: Rect) {
        let (node_clip, children) = {
            let Some(node) = self.nodes.get_mut(id.0) else {
                return;
            };
            node.clip_rect = node
                .clip_rect
                .intersection(clip)
                .unwrap_or(Rect::new(clip.x, clip.y, 0, 0));
            (node.clip_rect, node.children.clone())
        };
        for child in children {
            self.clip_subtree(child, node_clip);
        }
    }

    fn build_kind(
        &mut self,
        kind: &ViewKind,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        fill_height: bool,
        clip: Rect,
    ) -> (Vec<LayoutNodeId>, Size, bool) {
        match kind {
            ViewKind::Text(_) | ViewKind::Spacer { .. } => {
                let (size, fits) = match kind {
                    ViewKind::Text(text) => (
                        crate::presentation::layout::measure::text_intrinsic_size(text, width),
                        crate::presentation::layout::measure::text_fits(text, width),
                    ),
                    ViewKind::Spacer { rows } => (Size::new(0, (*rows).min(height)), true),
                    _ => unreachable!(),
                };
                (Vec::new(), size, fits && size.height <= height)
            }
            ViewKind::Container(ContainerNode { child }) => {
                let (id, size, complete) = self.build(child, x, y, Some(width), Some(height), clip);
                (vec![id], size, complete)
            }
            ViewKind::ClampRows(clamp) => {
                let (id, mut size, child_complete) =
                    self.build(&clamp.child, x, y, Some(width), None, clip);
                size.height = size.height.min(clamp.max_rows);
                let clamp_rect = Rect::new(x, y, width, size.height);
                self.clip_subtree(id, clamp_rect);
                (vec![id], size, child_complete)
            }
            ViewKind::RowViewport(viewport) => {
                let (id, child_size, child_complete) =
                    self.build(&viewport.child, x, y, Some(width), None, clip);
                (
                    vec![id],
                    Size::new(width, child_size.height.saturating_sub(viewport.skip_rows)),
                    child_complete,
                )
            }
            ViewKind::Column(column) => {
                self.build_column(column, x, y, width, height, fill_height, clip)
            }
            ViewKind::Row(row) => self.build_row(row, x, y, width, height, fill_height, clip),
            ViewKind::ComponentSlot(_) => unreachable!("component slot reached layout"),
        }
    }

    fn build_column(
        &mut self,
        column: &ColumnView,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        _fill_height: bool,
        clip: Rect,
    ) -> (Vec<LayoutNodeId>, Size, bool) {
        let tracks = column
            .children
            .iter()
            .map(|child| child.track)
            .collect::<Vec<_>>();
        let allocation = allocate_tracks(height, column.gap, &tracks, |index, _| {
            intrinsic_size(&column.children[index].view, width).height
        });
        let mut children = Vec::new();
        let mut cursor_y = y;
        let mut complete = true;
        for (index, child) in column.children.iter().enumerate() {
            let track = allocation.tracks[index];
            let (id, _size, child_complete) =
                self.build(&child.view, x, cursor_y, Some(width), Some(track), clip);
            children.push(id);
            complete &= child_complete;
            cursor_y = cursor_y
                .saturating_add(track)
                .saturating_add(allocation.gap);
            if intrinsic_size(&child.view, width).height > track
                && !matches!(&child.view.kind, ViewKind::ClampRows(_))
            {
                complete = false;
            }
        }
        let used_height = allocation
            .tracks
            .iter()
            .map(|track| usize::from(*track))
            .sum::<usize>()
            .saturating_add(usize::from(allocation.gap) * tracks.len().saturating_sub(1));
        (
            children,
            Size::new(width, used_height.min(usize::from(u16::MAX)) as u16),
            complete,
        )
    }

    fn build_row(
        &mut self,
        row: &RowView,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        fill_height: bool,
        clip: Rect,
    ) -> (Vec<LayoutNodeId>, Size, bool) {
        let tracks = row
            .children
            .iter()
            .map(|child| child.track)
            .collect::<Vec<_>>();
        let allocation = allocate_tracks(width, row.gap, &tracks, |index, remaining| {
            intrinsic_content_size(&row.children[index].view, remaining).width
        });
        let intrinsic_height = row
            .children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                intrinsic_size(
                    &child.view,
                    allocation.tracks.get(index).copied().unwrap_or(width),
                )
                .height
            })
            .max()
            .unwrap_or(0);
        let row_height = if fill_height {
            height
        } else {
            intrinsic_height.min(height)
        };
        let mut children = Vec::new();
        let mut cursor_x = x;
        let mut complete = true;
        for (index, child) in row.children.iter().enumerate() {
            let track = allocation.tracks[index];
            let child_height = intrinsic_size(&child.view, track).height.min(row_height);
            let child_y = match row.vertical_align {
                crate::presentation::VerticalAlign::Top => y,
                crate::presentation::VerticalAlign::Center => {
                    y.saturating_add(row_height.saturating_sub(child_height) / 2)
                }
                crate::presentation::VerticalAlign::Bottom => {
                    y.saturating_add(row_height.saturating_sub(child_height))
                }
            };
            let (id, _, child_complete) = self.build(
                &child.view,
                cursor_x,
                child_y,
                Some(track),
                Some(row_height),
                clip,
            );
            children.push(id);
            complete &= child_complete;
            cursor_x = cursor_x
                .saturating_add(track)
                .saturating_add(allocation.gap);
        }
        (children, Size::new(width, row_height), complete)
    }
}
