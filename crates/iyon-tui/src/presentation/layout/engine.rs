use crate::{
    geometry::{AxisConstraint, LayoutConstraints, Rect, Size},
    presentation::ir::{ColumnView, ContainerNode, HeightRule, RowView, View, ViewKind, WidthRule},
};

use super::{
    measure::{text_fits, text_intrinsic_size},
    tracks::allocate_tracks,
    tree::{LayoutNode, LayoutNodeId, LayoutPayload, LayoutTree},
};

#[derive(Clone, Copy)]
struct DecorationMetrics {
    left_border: u16,
    top_border: u16,
    left_padding: u16,
    horizontal: u16,
    vertical: u16,
    inner_width: u16,
}

fn decoration_metrics(view: &View, width: u16) -> DecorationMetrics {
    let (left_border, right_border, top_border, bottom_border) = view
        .decoration
        .border
        .as_ref()
        .map_or((0, 0, 0, 0), |border| {
            (
                border.left_width(),
                border.right_width(),
                border.top_height(),
                border.bottom_height(),
            )
        });
    let border_width = left_border.saturating_add(right_border);
    let padding_capacity = width.saturating_sub(border_width);
    let left_padding = view
        .decoration
        .padding
        .left
        .min(padding_capacity.saturating_sub(1));
    let right_padding = view
        .decoration
        .padding
        .right
        .min(padding_capacity.saturating_sub(left_padding.saturating_add(1)));
    let horizontal = border_width
        .saturating_add(left_padding)
        .saturating_add(right_padding);
    let vertical = top_border
        .saturating_add(bottom_border)
        .saturating_add(view.decoration.padding.top)
        .saturating_add(view.decoration.padding.bottom);
    DecorationMetrics {
        left_border,
        top_border,
        left_padding,
        horizontal,
        vertical,
        inner_width: width.saturating_sub(horizontal),
    }
}

#[derive(Clone, Copy)]
enum WidthIntent {
    Semantic,
    ForceFit,
}

#[derive(Debug, Default)]
struct LayoutPass {
    nodes: Vec<LayoutNode>,
    physically_complete: bool,
}

pub(crate) fn layout_view(view: &View, constraints: LayoutConstraints) -> LayoutTree {
    let mut pass = LayoutPass::default();
    let width = constraints
        .width
        .definite()
        .unwrap_or_else(|| pass.measure(view, u16::MAX).width);
    let height = constraints.height.definite();
    let clip = Rect::new(0, 0, width, height.unwrap_or(u16::MAX));
    let (root, size, complete) = pass.build(view, 0, 0, Some(width), height, clip);
    pass.physically_complete = complete;
    let mut tree = LayoutTree {
        root,
        nodes: pass.nodes,
        size,
        physically_complete: pass.physically_complete,
    };
    // A width-only layout has no synthetic infinite height in its public result.
    if matches!(constraints.height, AxisConstraint::Unbounded) {
        tree.size.height = tree.node(root).rect.height;
    }
    debug_assert!(tree.validate(), "invalid layout tree: {tree:?}");
    tree
}

pub(crate) fn measure_view(view: &View, width: u16) -> Size {
    LayoutPass::default().measure(view, width)
}

fn track_intrinsic_height(track: crate::presentation::ir::TrackSize, height: u16) -> u16 {
    match track {
        crate::presentation::ir::TrackSize::Fixed(value) => value,
        crate::presentation::ir::TrackSize::Content { max } => {
            max.map_or(height, |value| height.min(value))
        }
        crate::presentation::ir::TrackSize::Flex { .. } => height,
        crate::presentation::ir::TrackSize::FlexMax { max, .. } => height.min(max),
    }
}

fn column_gap(gap: u16, count: usize) -> u16 {
    gap.saturating_mul(count.saturating_sub(1).min(usize::from(u16::MAX)) as u16)
}

impl LayoutPass {
    fn measure(&mut self, view: &View, width: u16) -> Size {
        self.measure_with_intent(view, width, WidthIntent::Semantic)
    }

    fn measure_content(&mut self, view: &View, width: u16) -> Size {
        self.measure_with_intent(view, width, WidthIntent::ForceFit)
    }

    fn measure_with_intent(&mut self, view: &View, width: u16, intent: WidthIntent) -> Size {
        let bounds = view.decoration.bounds;
        let width = width.min(bounds.width.normalized_max());
        let metrics = decoration_metrics(view, width);
        let core = self.measure_kind(view, metrics.inner_width);
        let core_width = match (intent, view.width) {
            (WidthIntent::ForceFit, _) | (_, WidthRule::Fit) => core.width,
            (_, WidthRule::Fill) => metrics.inner_width,
        };
        let outer_width = core_width
            .saturating_add(metrics.horizontal)
            .max(bounds.width.min)
            .min(width);
        let outer_height = core
            .height
            .saturating_add(metrics.vertical)
            .max(bounds.height.min)
            .min(bounds.height.normalized_max());
        Size::new(outer_width, outer_height)
    }

    fn measure_kind(&mut self, view: &View, width: u16) -> Size {
        match &view.kind {
            ViewKind::Text(text) => text_intrinsic_size(text, width),
            ViewKind::Spacer { rows } => Size::new(0, *rows),
            ViewKind::Container(container) => self.measure(&container.child, width),
            ViewKind::Hanging(hanging) => {
                let prefix_width = self.measure_content(&hanging.prefix, u16::MAX).width;
                let body_width = width.saturating_sub(prefix_width).max(1);
                let body = self.measure(&hanging.body, body_width);
                Size::new(prefix_width.saturating_add(body.width), body.height.max(1))
            }
            ViewKind::ClampRows(clamp) => {
                let mut size = self.measure(&clamp.child, width);
                size.height = size.height.min(clamp.max_rows);
                size
            }
            ViewKind::RowViewport(viewport) => {
                let child = self.measure(&viewport.child, width);
                Size::new(
                    width,
                    if viewport.intrinsic_content_height {
                        child.height
                    } else {
                        viewport
                            .visible_height
                            .unwrap_or_else(|| child.height.saturating_sub(viewport.skip_rows))
                    },
                )
            }
            ViewKind::Column(column) => {
                let children = column
                    .children
                    .iter()
                    .map(|child| (child, self.measure(&child.view, width)))
                    .collect::<Vec<_>>();
                let height = children
                    .iter()
                    .map(|(child, size)| track_intrinsic_height(child.track, size.height))
                    .map(usize::from)
                    .sum::<usize>()
                    .saturating_add(usize::from(column_gap(column.gap, children.len())));
                Size::new(
                    children
                        .iter()
                        .map(|(_, size)| size.width)
                        .max()
                        .unwrap_or(0),
                    height.min(usize::from(u16::MAX)) as u16,
                )
            }
            ViewKind::Row(row) => {
                let tracks = row
                    .children
                    .iter()
                    .map(|child| child.track)
                    .collect::<Vec<_>>();
                let allocation = allocate_tracks(width, row.gap, &tracks, |index, remaining| {
                    self.measure_content(&row.children[index].view, remaining)
                        .width
                });
                let height = row
                    .children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| {
                        self.measure(&child.view, allocation.tracks[index]).height
                    })
                    .max()
                    .unwrap_or(0);
                Size::new(
                    allocation
                        .tracks
                        .iter()
                        .copied()
                        .sum::<u16>()
                        .saturating_add(column_gap(row.gap, allocation.tracks.len())),
                    height,
                )
            }
            ViewKind::ComponentSlot(_) => unreachable!("component slot reached measurement"),
        }
    }

    fn build(
        &mut self,
        view: &View,
        x: u16,
        y: u16,
        width_bound: Option<u16>,
        height_bound: Option<u16>,
        clip: Rect,
    ) -> (LayoutNodeId, Size, bool) {
        let width_capacity = width_bound
            .unwrap_or(u16::MAX)
            .min(view.decoration.bounds.width.normalized_max());
        let metrics = decoration_metrics(view, width_capacity);
        let left_padding = metrics.left_padding;
        let horizontal_decoration = metrics.horizontal;
        let vertical_decoration = metrics.vertical;
        let inner_width_bound = metrics.inner_width;
        let intrinsic = self.measure(view, width_capacity);
        let intrinsic_core = Size::new(
            intrinsic.width.saturating_sub(horizontal_decoration),
            intrinsic.height.saturating_sub(vertical_decoration),
        );
        let minimum_core_width = view
            .decoration
            .bounds
            .width
            .min
            .saturating_sub(horizontal_decoration)
            .min(inner_width_bound);
        let core_width = match view.width {
            WidthRule::Fill => inner_width_bound,
            WidthRule::Fit => intrinsic_core.width.min(inner_width_bound),
        }
        .max(minimum_core_width);
        let height_capacity = height_bound
            .unwrap_or(u16::MAX)
            .min(view.decoration.bounds.height.normalized_max());
        let core_height_bound = height_capacity.saturating_sub(vertical_decoration);
        let minimum_core_height = view
            .decoration
            .bounds
            .height
            .min
            .saturating_sub(vertical_decoration)
            .min(core_height_bound);
        let requested_core_height = match (view.height, height_bound) {
            (HeightRule::Fill, Some(_)) => core_height_bound,
            _ => intrinsic_core.height.min(core_height_bound),
        }
        .max(minimum_core_height);
        let content_x = x
            .saturating_add(metrics.left_border)
            .saturating_add(left_padding);
        let content_y = y
            .saturating_add(metrics.top_border)
            .saturating_add(view.decoration.padding.top);
        let (children, core_size, complete) = self.build_kind(
            &view.kind,
            content_x,
            content_y,
            core_width,
            requested_core_height,
            view.height == HeightRule::Fill,
            height_bound.is_some(),
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
                .max(view.decoration.bounds.width.min)
                .min(width_capacity),
            core_height
                .saturating_add(vertical_decoration)
                .max(view.decoration.bounds.height.min)
                .min(height_capacity),
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
            if !matches!(view.kind, ViewKind::RowViewport(_)) {
                self.clip_subtree(child, node_clip);
            }
        }
        (id, size, complete)
    }

    fn clip_subtree(&mut self, id: LayoutNodeId, clip: Rect) {
        let (node_clip, children, is_viewport) = {
            let Some(node) = self.nodes.get_mut(id.0) else {
                return;
            };
            node.clip_rect = node
                .clip_rect
                .intersection(clip)
                .unwrap_or(Rect::new(clip.x, clip.y, 0, 0));
            let is_viewport = matches!(node.view.kind, ViewKind::RowViewport(_));
            (node.clip_rect, node.children.clone(), is_viewport)
        };
        if is_viewport {
            return;
        }
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
        bounded_height: bool,
        clip: Rect,
    ) -> (Vec<LayoutNodeId>, Size, bool) {
        match kind {
            ViewKind::Text(_) | ViewKind::Spacer { .. } => {
                let (size, fits) = match kind {
                    ViewKind::Text(text) => {
                        (text_intrinsic_size(text, width), text_fits(text, width))
                    }
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
                let child_clip = Rect::new(clip.x, 0, clip.width, u16::MAX);
                let (id, child_size, child_complete) = self.build(
                    &viewport.child,
                    x,
                    y,
                    Some(width),
                    viewport.layout_height,
                    child_clip,
                );
                (
                    vec![id],
                    Size::new(
                        width,
                        if viewport.intrinsic_content_height {
                            let remaining = child_size.height.saturating_sub(viewport.skip_rows);
                            if bounded_height {
                                height.min(remaining)
                            } else {
                                child_size.height
                            }
                        } else {
                            viewport.visible_height.unwrap_or_else(|| {
                                child_size.height.saturating_sub(viewport.skip_rows)
                            })
                        },
                    ),
                    child_complete,
                )
            }
            ViewKind::Column(column) => {
                self.build_column(column, x, y, width, height, fill_height, clip)
            }
            ViewKind::Row(row) => self.build_row(row, x, y, width, height, fill_height, clip),
            ViewKind::Hanging(hanging) => {
                self.build_hanging(hanging, x, y, width, height, fill_height, clip)
            }
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
            self.measure(&column.children[index].view, width).height
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
            if self.measure(&child.view, width).height > track
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

    fn build_hanging(
        &mut self,
        hanging: &crate::presentation::ir::HangingView,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        fill_height: bool,
        clip: Rect,
    ) -> (Vec<LayoutNodeId>, Size, bool) {
        if height == 0 {
            return (Vec::new(), Size::new(width, 0), true);
        }

        let prefix_width = self.measure_content(&hanging.prefix, u16::MAX).width;
        let body_width = width.saturating_sub(prefix_width).max(1);
        let (body_id, body_size, body_complete) = self.build(
            &hanging.body,
            x.saturating_add(prefix_width),
            y,
            Some(body_width),
            Some(height),
            clip,
        );
        let row_height = body_size.height.max(1).min(height);
        let (prefix_id, _, prefix_complete) =
            self.build(&hanging.prefix, x, y, Some(prefix_width), Some(1), clip);
        let mut children = vec![prefix_id];
        let mut complete = body_complete && prefix_complete && prefix_width < width;
        for row in 1..row_height {
            let (continuation_id, _, continuation_complete) = self.build(
                &hanging.continuation_prefix,
                x,
                y.saturating_add(row),
                Some(prefix_width),
                Some(1),
                clip,
            );
            children.push(continuation_id);
            complete &= continuation_complete;
        }
        children.push(body_id);
        let output_height = if fill_height { height } else { row_height };
        (
            children,
            Size::new(width, output_height),
            complete && body_size.height <= height,
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
            self.measure_content(&row.children[index].view, remaining)
                .width
        });
        let intrinsic_height = row
            .children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                self.measure(
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
            let child_height = self.measure(&child.view, track).height.min(row_height);
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
