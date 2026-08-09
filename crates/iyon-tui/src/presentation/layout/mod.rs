//! Private semantic layout/compiler boundary.
//!
//! This module owns backend-neutral constraints, retained layout geometry, and
//! the compatibility compiler facade. Physical lowering lives in painting.

mod engine;
mod measure;
mod tracks;
mod tree;

#[cfg(test)]
mod tests;

use crate::{
    geometry::{LayoutConstraints, Size},
    physical::{PhysicalRow, PhysicalStyle, Surface},
    presentation::View,
};

pub(crate) use engine::{LayoutEngine, ManualLayoutEngine};
pub(crate) use tree::{
    ComponentGeometry, ComponentGeometryMap, LayoutNode, LayoutNodeId, LayoutPayload, LayoutTree,
};

use super::paint::{ThemeResolver, ViewPainter};

pub(crate) use super::paint::{CompiledTextRow, row_from_graphemes, row_from_string};
pub(crate) use tracks::allocate_tracks;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayoutBlock {
    pub(crate) width: u16,
    pub(crate) rows: Vec<PhysicalRow>,
    pub(crate) physically_complete: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ViewCompiler {
    pub(crate) theme: ThemeResolver,
}

impl ViewCompiler {
    pub(crate) fn layout(&self, view: &View, max_width: u16, inherited: PhysicalStyle) -> Surface {
        let tree = self.layout_tree(view, LayoutConstraints::width_only(max_width));
        ViewPainter.paint_tree_with_style(self, &tree, inherited)
    }

    pub(crate) fn compile(&self, view: &View, max_width: u16) -> LayoutBlock {
        let tree = self.layout_tree(view, LayoutConstraints::width_only(max_width));
        let surface = ViewPainter.paint_tree(self, &tree);
        let physically_complete = surface.physically_complete;
        LayoutBlock {
            width: surface.width(),
            rows: lower_surface(surface),
            physically_complete,
        }
    }

    pub(crate) fn layout_tree(&self, view: &View, constraints: LayoutConstraints) -> LayoutTree {
        ManualLayoutEngine.layout(view, constraints)
    }

    pub(crate) fn compile_bounded(&self, view: &View, size: Size) -> LayoutBlock {
        let tree = self.layout_tree(view, LayoutConstraints::bounded(size));
        let surface = ViewPainter.paint_tree(self, &tree);
        let height = surface.height().min(size.height);
        let mut bounded = Surface::new(surface.width().min(size.width), height);
        bounded.physically_complete = tree.physically_complete && surface.physically_complete;
        for y in 0..bounded.height() {
            for x in 0..bounded.width() {
                *bounded.get_mut(x, y) = surface.get(x, y).clone();
            }
        }
        let physically_complete = bounded.physically_complete;
        LayoutBlock {
            width: bounded.width(),
            rows: lower_surface(bounded),
            physically_complete,
        }
    }
}

pub(crate) fn compile_view(view: &View, width: u16) -> LayoutBlock {
    ViewCompiler::default().compile(view, width)
}

pub(crate) fn view_height(view: &View, width: u16) -> u16 {
    compile_view(view, width)
        .rows
        .len()
        .min(usize::from(u16::MAX)) as u16
}

fn lower_surface(surface: Surface) -> Vec<PhysicalRow> {
    (0..surface.height())
        .map(|y| {
            let cells = (0..surface.width())
                .map(|x| surface.get(x, y).clone())
                .collect();
            PhysicalRow::from_cells(cells)
        })
        .collect()
}
