//! Private semantic layout/compiler boundary.
//!
//! Manual layout and text compilation produce backend-neutral physical rows.
//! Terminal adapters are deliberately outside this module.

mod manual;
mod text;
mod tracks;

#[cfg(test)]
mod tests;

use crate::{
    physical::{PhysicalRow, PhysicalStyle, Surface},
    presentation::View,
};

use super::paint::ThemeResolver;

pub(crate) use tracks::allocate_tracks;

#[derive(Clone, Debug)]
pub(crate) struct LayoutBlock {
    pub(crate) width: u16,
    pub(crate) rows: Vec<PhysicalRow>,
    pub(crate) physically_complete: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ViewCompiler {
    pub(crate) theme: ThemeResolver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledTextRow {
    pub(crate) row: PhysicalRow,
    pub(crate) source_end: Option<usize>,
    pub(crate) fits: bool,
    pub(crate) width: usize,
}

impl ViewCompiler {
    pub(crate) fn compile(&self, view: &View, max_width: u16) -> LayoutBlock {
        let surface = self.layout(view, max_width, PhysicalStyle::default());
        let physically_complete = surface.physically_complete;
        LayoutBlock {
            width: surface.width,
            rows: lower_surface(surface),
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
    (0..surface.height)
        .map(|y| {
            let cells = (0..surface.width)
                .map(|x| surface.get(x, y).clone())
                .collect();
            PhysicalRow::from_cells(cells)
        })
        .collect()
}
