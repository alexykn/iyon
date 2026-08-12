//! Private semantic layout/compiler boundary.
//!
//! This module owns backend-neutral constraints, retained layout geometry, and
//! the compiler facade. Physical lowering lives in painting.
//!
//! Layout is a three-stage pipeline:
//!
//! 1. Measure semantic Views into width-dependent MeasuredNodes.
//! 2. Resolve bounded allocation into PreparedNodes using only measured facts.
//! 3. Place PreparedNodes into a LayoutTree without performing measurement or
//!    layout allocation.
//!
//! A semantic View subtree must never be re-measured merely because placement
//! needs geometry, and LayoutTree must not retain recursive clones of semantic
//! View subtrees.

mod engine;
mod measure;
mod place;
mod prepare;
mod tracks;
mod tree;

#[cfg(test)]
mod tests;

use crate::{
    Theme,
    component::{ComponentId, MountGraph},
    geometry::LayoutConstraints,
    physical::{PhysicalRow, Surface},
    presentation::View,
};

pub(crate) use engine::{layout_view, measure_view};
pub(crate) use tree::{ComponentGeometryMap, LayoutContent, LayoutNode, LayoutNodeId, LayoutTree};

#[cfg(test)]
use crate::geometry::Size;

use super::paint::{StyleContext, ThemeResolver, ViewPainter};

pub(crate) use super::paint::{CompiledTextRow, row_from_graphemes, row_from_string};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static MEASURE_NODES: Cell<usize> = const { Cell::new(0) };
    static PREPARE_NODES: Cell<usize> = const { Cell::new(0) };
    static EMITTED_NODES: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(super) fn record_measure_node() {
    MEASURE_NODES.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(super) fn record_prepare_node() {
    PREPARE_NODES.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(super) fn record_emitted_node() {
    EMITTED_NODES.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(super) fn reset_layout_counters() {
    MEASURE_NODES.with(|count| count.set(0));
    PREPARE_NODES.with(|count| count.set(0));
    EMITTED_NODES.with(|count| count.set(0));
}

#[cfg(test)]
pub(super) fn layout_counters() -> (usize, usize, usize) {
    (
        MEASURE_NODES.with(Cell::get),
        PREPARE_NODES.with(Cell::get),
        EMITTED_NODES.with(Cell::get),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayoutBlock {
    pub(crate) width: u16,
    pub(crate) rows: Vec<PhysicalRow>,
    pub(crate) physically_complete: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ViewCompiler {
    pub(crate) theme: ThemeResolver,
    pub(crate) focused: Option<ComponentId>,
    pub(crate) graph: Option<MountGraph>,
}

impl ViewCompiler {
    pub(crate) fn new(theme: &Theme) -> Self {
        Self {
            theme: ThemeResolver::new(theme),
            focused: None,
            graph: None,
        }
    }

    pub(crate) fn with_interaction(
        theme: &Theme,
        focused: Option<ComponentId>,
        graph: &MountGraph,
    ) -> Self {
        Self {
            theme: ThemeResolver::new(theme),
            focused,
            graph: Some(graph.clone()),
        }
    }

    pub(crate) fn with_resolver(theme: &ThemeResolver) -> Self {
        Self {
            theme: theme.clone(),
            focused: None,
            graph: None,
        }
    }

    pub(crate) fn style_context(&self, scope: Option<ComponentId>) -> StyleContext {
        StyleContext::for_scope(scope, self.focused, self.graph.as_ref())
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
        layout_view(view, constraints)
    }
}

#[cfg(test)]
pub(crate) fn compile_view(view: &View, width: u16) -> LayoutBlock {
    compile_view_with_theme(view, width, &Theme::default())
}

pub(crate) fn compile_view_with_theme(view: &View, width: u16, theme: &Theme) -> LayoutBlock {
    ViewCompiler::new(theme).compile(view, width)
}

#[cfg(test)]
pub(crate) fn compile_bounded_view(view: &View, size: Size) -> LayoutBlock {
    let compiler = ViewCompiler::default();
    let tree = compiler.layout_tree(view, LayoutConstraints::bounded(size));
    let surface = ViewPainter.paint_tree(&compiler, &tree);
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
