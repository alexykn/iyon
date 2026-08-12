//! Backend-neutral semantic measurement used by the layout pipeline.

use crate::{
    geometry::Size,
    presentation::{
        ir::{
            ColumnView, ContainerNode, HangingView, RowView, RowViewportView, TrackSize, View,
            ViewKind, WidthRule,
        },
        wrap::{TextFlowMetrics, text_flow_metrics},
    },
};

use super::tracks::{TrackAllocation, allocate_tracks};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DecorationMetrics {
    pub(super) left_border: u16,
    pub(super) right_border: u16,
    pub(super) top_border: u16,
    pub(super) bottom_border: u16,
    pub(super) left_padding: u16,
    pub(super) right_padding: u16,
    pub(super) top_padding: u16,
    pub(super) bottom_padding: u16,
    pub(super) horizontal: u16,
    pub(super) vertical: u16,
    pub(super) inner_width: u16,
}

pub(super) fn decoration_metrics(view: &View, width: u16) -> DecorationMetrics {
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
    let top_padding = view.decoration.padding.top;
    let bottom_padding = view.decoration.padding.bottom;
    let vertical = top_border
        .saturating_add(bottom_border)
        .saturating_add(top_padding)
        .saturating_add(bottom_padding);
    DecorationMetrics {
        left_border,
        right_border,
        top_border,
        bottom_border,
        left_padding,
        right_padding,
        top_padding,
        bottom_padding,
        horizontal,
        vertical,
        inner_width: width.saturating_sub(horizontal),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WidthIntent {
    Semantic,
    ForceFit,
}

#[derive(Debug)]
pub(super) struct MeasuredNode<'a> {
    pub(super) view: &'a View,
    pub(super) width_capacity: u16,
    pub(super) decoration: DecorationMetrics,
    pub(super) size: Size,
    pub(super) core_size: Size,
    pub(super) kind: MeasuredKind<'a>,
}

#[derive(Debug)]
pub(super) enum MeasuredKind<'a> {
    Text {
        text: &'a crate::presentation::ir::TextView,
        metrics: TextFlowMetrics,
    },
    Spacer {
        rows: u16,
    },
    Container {
        child: Box<MeasuredNode<'a>>,
    },
    Column {
        children: Vec<MeasuredColumnChild<'a>>,
        gap: u16,
    },
    Row {
        allocation: TrackAllocation,
        children: Vec<MeasuredRowChild<'a>>,
        gap: u16,
        vertical_align: crate::presentation::VerticalAlign,
    },
    Hanging {
        prefix_width: u16,
        prefix: Box<MeasuredNode<'a>>,
        continuation_prefix: Box<MeasuredNode<'a>>,
        body: Box<MeasuredNode<'a>>,
    },
    ClampRows {
        child: Box<MeasuredNode<'a>>,
        max_rows: u16,
        overflow: &'a crate::presentation::OverflowIndicator,
    },
    RowViewport {
        width: u16,
        child: Box<MeasuredNode<'a>>,
        skip_rows: u16,
        visible_height: Option<u16>,
        layout_height: Option<u16>,
        intrinsic_content_height: bool,
    },
}

#[derive(Debug)]
pub(super) struct MeasuredColumnChild<'a> {
    pub(super) track: TrackSize,
    pub(super) node: MeasuredNode<'a>,
}

#[derive(Debug)]
pub(super) struct MeasuredRowChild<'a> {
    pub(super) track_width: u16,
    pub(super) node: MeasuredNode<'a>,
}

pub(super) fn measure_node<'a>(
    view: &'a View,
    width: u16,
    intent: WidthIntent,
) -> MeasuredNode<'a> {
    #[cfg(test)]
    super::record_measure_node();
    let bounds = view.decoration.bounds;
    let width_capacity = width.min(bounds.width.normalized_max());
    let decoration = decoration_metrics(view, width_capacity);
    let kind = measure_kind(view, decoration.inner_width);
    let core_size = kind.intrinsic_size();
    let core_width = match (intent, view.width) {
        (WidthIntent::ForceFit, _) | (_, WidthRule::Fit) => core_size.width,
        (_, WidthRule::Fill) => decoration.inner_width,
    };
    let size = Size::new(
        core_width
            .saturating_add(decoration.horizontal)
            .max(bounds.width.min)
            .min(width_capacity),
        core_size
            .height
            .saturating_add(decoration.vertical)
            .max(bounds.height.min)
            .min(bounds.height.normalized_max()),
    );
    MeasuredNode {
        view,
        width_capacity,
        decoration,
        size,
        core_size,
        kind,
    }
}

fn measure_kind<'a>(view: &'a View, width: u16) -> MeasuredKind<'a> {
    match &view.kind {
        ViewKind::Text(text) => {
            let metrics = text_flow_metrics(text, width);
            MeasuredKind::Text { text, metrics }
        }
        ViewKind::Spacer { rows } => MeasuredKind::Spacer { rows: *rows },
        ViewKind::Container(ContainerNode { child }) => MeasuredKind::Container {
            child: Box::new(measure_node(child, width, WidthIntent::Semantic)),
        },
        ViewKind::Hanging(hanging) => measure_hanging(hanging, width),
        ViewKind::ClampRows(clamp) => {
            let child = Box::new(measure_node(&clamp.child, width, WidthIntent::Semantic));
            MeasuredKind::ClampRows {
                child,
                max_rows: clamp.max_rows,
                overflow: &clamp.overflow,
            }
        }
        ViewKind::RowViewport(viewport) => measure_viewport(viewport, width),
        ViewKind::Column(column) => measure_column(column, width),
        ViewKind::Row(row) => measure_row(row, width),
        ViewKind::ComponentSlot(_) => unreachable!("component slot reached measurement"),
    }
}

fn measure_hanging<'a>(hanging: &'a HangingView, width: u16) -> MeasuredKind<'a> {
    let prefix = Box::new(measure_node(
        &hanging.prefix,
        u16::MAX,
        WidthIntent::ForceFit,
    ));
    let prefix_width = prefix.size.width;
    let body_width = width.saturating_sub(prefix_width).max(1);
    let body = Box::new(measure_node(
        &hanging.body,
        body_width,
        WidthIntent::Semantic,
    ));
    let continuation_prefix = Box::new(measure_node(
        &hanging.continuation_prefix,
        prefix_width,
        WidthIntent::Semantic,
    ));
    MeasuredKind::Hanging {
        prefix_width,
        prefix,
        continuation_prefix,
        body,
    }
}

fn measure_column<'a>(column: &'a ColumnView, width: u16) -> MeasuredKind<'a> {
    let children = column
        .children
        .iter()
        .map(|child| MeasuredColumnChild {
            track: child.track,
            node: measure_node(&child.view, width, WidthIntent::Semantic),
        })
        .collect::<Vec<_>>();
    MeasuredKind::Column {
        children,
        gap: column.gap,
    }
}

fn measure_row<'a>(row: &'a RowView, width: u16) -> MeasuredKind<'a> {
    let tracks = row
        .children
        .iter()
        .map(|child| child.track)
        .collect::<Vec<_>>();
    let allocation = allocate_tracks(width, row.gap, &tracks, |index, remaining| {
        measure_node(&row.children[index].view, remaining, WidthIntent::ForceFit)
            .size
            .width
    });
    let children = row
        .children
        .iter()
        .enumerate()
        .map(|(index, child)| {
            let track_width = allocation.tracks[index];
            MeasuredRowChild {
                track_width,
                node: measure_node(&child.view, track_width, WidthIntent::Semantic),
            }
        })
        .collect::<Vec<_>>();
    MeasuredKind::Row {
        allocation,
        children,
        gap: row.gap,
        vertical_align: row.vertical_align,
    }
}

fn measure_viewport<'a>(viewport: &'a RowViewportView, width: u16) -> MeasuredKind<'a> {
    let child = Box::new(measure_node(&viewport.child, width, WidthIntent::Semantic));
    MeasuredKind::RowViewport {
        width,
        child,
        skip_rows: viewport.skip_rows,
        visible_height: viewport.visible_height,
        layout_height: viewport.layout_height,
        intrinsic_content_height: viewport.intrinsic_content_height,
    }
}

impl MeasuredKind<'_> {
    pub(super) fn intrinsic_size(&self) -> Size {
        match self {
            Self::Text { metrics, .. } => Size::new(metrics.width, metrics.row_count),
            Self::Spacer { rows } => Size::new(0, *rows),
            Self::Container { child } => child.size,
            Self::Hanging {
                prefix_width, body, ..
            } => Size::new(
                prefix_width.saturating_add(body.size.width),
                body.size.height.max(1),
            ),
            Self::ClampRows {
                child, max_rows, ..
            } => Size::new(child.size.width, child.size.height.min(*max_rows)),
            Self::RowViewport {
                width,
                child,
                skip_rows,
                visible_height,
                intrinsic_content_height,
                ..
            } => Size::new(
                *width,
                if *intrinsic_content_height {
                    child.size.height
                } else {
                    visible_height.unwrap_or_else(|| child.size.height.saturating_sub(*skip_rows))
                },
            ),
            Self::Column { children, gap } => {
                let width = children
                    .iter()
                    .map(|child| child.node.size.width)
                    .max()
                    .unwrap_or(0);
                let height = children
                    .iter()
                    .map(|child| track_intrinsic_height(child.track, child.node.size.height))
                    .map(usize::from)
                    .sum::<usize>()
                    .saturating_add(usize::from(column_gap(*gap, children.len())));
                Size::new(width, height.min(usize::from(u16::MAX)) as u16)
            }
            Self::Row {
                allocation,
                children,
                gap,
                ..
            } => Size::new(
                allocation
                    .tracks
                    .iter()
                    .copied()
                    .sum::<u16>()
                    .saturating_add(column_gap(*gap, allocation.tracks.len())),
                children
                    .iter()
                    .map(|child| child.node.size.height)
                    .max()
                    .unwrap_or(0),
            ),
        }
    }

    pub(super) fn is_clamp(&self) -> bool {
        matches!(self, Self::ClampRows { .. })
    }
}

fn track_intrinsic_height(track: TrackSize, height: u16) -> u16 {
    match track {
        TrackSize::Fixed(value) => value,
        TrackSize::Content { max } => max.map_or(height, |value| height.min(value)),
        TrackSize::Flex { .. } => height,
        TrackSize::FlexMax { max, .. } => height.min(max),
    }
}

fn column_gap(gap: u16, count: usize) -> u16 {
    gap.saturating_mul(count.saturating_sub(1).min(usize::from(u16::MAX)) as u16)
}
