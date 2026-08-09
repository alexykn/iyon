//! Backend-neutral intrinsic measurement for the manual layout engine.

use crate::{
    geometry::Size,
    presentation::{
        ir::{TrackSize, View, ViewKind},
        wrap::text_flow_metrics,
    },
};

pub(crate) fn horizontal_decoration(view: &View, width: u16) -> (u16, u16, u16) {
    let decoration = &view.decoration;
    let border = u16::from(decoration.border.is_some());
    let border_pad = border.saturating_mul(2);
    let max_content = width.saturating_sub(border_pad);
    let left = decoration.padding.left.min(max_content.saturating_sub(1));
    let right = decoration
        .padding
        .right
        .min(max_content.saturating_sub(left.saturating_add(1)));
    (
        left,
        right,
        left.saturating_add(right).saturating_add(border_pad),
    )
}

pub(crate) fn intrinsic_size(view: &View, width: u16) -> Size {
    let decoration = &view.decoration;
    let (_, _, horizontal) = horizontal_decoration(view, width);
    let border = u16::from(decoration.border.is_some());
    let inner_width = width.saturating_sub(horizontal);
    let core = match &view.kind {
        ViewKind::Text(text) => text_intrinsic_size(text, inner_width),
        ViewKind::Spacer { rows } => Size::new(0, *rows),
        ViewKind::Container(container) => intrinsic_size(&container.child, inner_width),
        ViewKind::ClampRows(clamp) => {
            let mut size = intrinsic_size(&clamp.child, inner_width);
            size.height = size.height.min(clamp.max_rows);
            size
        }
        ViewKind::RowViewport(viewport) => {
            let child = intrinsic_size(&viewport.child, inner_width);
            Size::new(
                inner_width,
                viewport
                    .visible_height
                    .unwrap_or_else(|| child.height.saturating_sub(viewport.skip_rows)),
            )
        }
        ViewKind::Column(column) => {
            let children = column
                .children
                .iter()
                .map(|child| (child, intrinsic_size(&child.view, inner_width)))
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
            let allocation = super::tracks::allocate_tracks(
                inner_width,
                row.gap,
                &tracks,
                |index, remaining| {
                    intrinsic_content_size(&row.children[index].view, remaining).width
                },
            );
            let height = row
                .children
                .iter()
                .enumerate()
                .map(|(index, child)| intrinsic_size(&child.view, allocation.tracks[index]).height)
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
    };

    let core_width = if view.width == crate::presentation::ir::WidthRule::Fill {
        inner_width
    } else {
        core.width
    };
    Size::new(
        core_width
            .saturating_add(decoration.padding.left)
            .saturating_add(decoration.padding.right)
            .saturating_add(border.saturating_mul(2)),
        core.height
            .saturating_add(decoration.padding.top)
            .saturating_add(decoration.padding.bottom)
            .saturating_add(border.saturating_mul(2)),
    )
}

fn track_intrinsic_height(track: TrackSize, height: u16) -> u16 {
    match track {
        TrackSize::Fixed(value) => value,
        TrackSize::Content { max } => max.map_or(height, |value| height.min(value)),
        TrackSize::Flex { .. } => height,
    }
}

fn column_gap(gap: u16, count: usize) -> u16 {
    gap.saturating_mul(count.saturating_sub(1).min(usize::from(u16::MAX)) as u16)
}

pub(crate) fn intrinsic_content_size(view: &View, width: u16) -> Size {
    let mut view = view.clone();
    view.width = crate::presentation::ir::WidthRule::Fit;
    intrinsic_size(&view, width)
}

pub(crate) fn text_intrinsic_size(text: &crate::presentation::ir::TextView, width: u16) -> Size {
    let flow = text_flow_metrics(text, width);
    Size::new(flow.width, flow.row_count)
}

pub(crate) fn text_fits(text: &crate::presentation::ir::TextView, width: u16) -> bool {
    text_flow_metrics(text, width).fits
}
