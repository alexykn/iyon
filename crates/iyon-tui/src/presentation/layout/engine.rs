//! Layout orchestration for the measure, prepare, and placement pipeline.

use crate::geometry::{AxisConstraint, LayoutConstraints, Point, Rect, Size};
use crate::presentation::ir::View;
use crate::scene::ResolutionOverlay;

use super::{measure::measure_node, place::emit_prepared, prepare::prepare_node};

pub(crate) fn layout_view(view: &View, constraints: LayoutConstraints) -> super::tree::LayoutTree {
    layout_view_with_overlay(view, constraints, &ResolutionOverlay::default())
}

pub(crate) fn layout_view_with_overlay(
    view: &View,
    constraints: LayoutConstraints,
    overlay: &ResolutionOverlay,
) -> super::tree::LayoutTree {
    let width = constraints.width.definite().unwrap_or_else(|| {
        measure_node(
            view,
            u16::MAX,
            super::measure::WidthIntent::Semantic,
            overlay,
            None,
        )
        .size
        .width
    });
    let measured = measure_node(
        view,
        width,
        super::measure::WidthIntent::Semantic,
        overlay,
        None,
    );
    let prepared = prepare_node(&measured, constraints.height.definite());
    let root_clip = Rect::new(
        0,
        0,
        prepared.size.width,
        constraints
            .height
            .definite()
            .unwrap_or(prepared.size.height),
    );
    let mut nodes = Vec::new();
    let root = emit_prepared(&prepared, Point::default(), root_clip, &mut nodes);
    let mut tree = super::tree::LayoutTree {
        root,
        nodes,
        size: prepared.size,
        physically_complete: prepared.complete,
    };
    if matches!(constraints.height, AxisConstraint::Unbounded) {
        tree.size.height = tree.node(root).rect.height;
    }
    debug_assert!(tree.validate(), "invalid layout tree: {tree:?}");
    tree
}

pub(crate) fn measure_view(view: &View, width: u16) -> Size {
    measure_view_with_overlay(view, width, &ResolutionOverlay::default())
}

pub(crate) fn measure_view_with_overlay(
    view: &View,
    width: u16,
    overlay: &ResolutionOverlay,
) -> Size {
    measure_node(
        view,
        width,
        super::measure::WidthIntent::Semantic,
        overlay,
        None,
    )
    .size
}
