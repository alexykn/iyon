//! Immutable final physical rows.

use super::{PhysicalCell, PhysicalStyle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhysicalRow {
    cells: Vec<PhysicalCell>,
}

impl PhysicalRow {
    pub(crate) fn from_cells(cells: Vec<PhysicalCell>) -> Self {
        Self { cells }
    }

    pub(crate) fn empty() -> Self {
        Self::from_cells(Vec::new())
    }

    pub(crate) fn width(&self) -> usize {
        self.cells.len()
    }

    pub(crate) fn cells(&self) -> &[PhysicalCell] {
        &self.cells
    }

    pub(crate) fn cell(&self, index: usize) -> Option<&PhysicalCell> {
        self.cells.get(index)
    }

    pub(crate) fn plain_text(&self) -> String {
        let last_painted = self
            .cells
            .iter()
            .rposition(|cell| cell.painted && !cell.continuation);
        let Some(last_painted) = last_painted else {
            return String::new();
        };

        self.cells[..=last_painted]
            .iter()
            .filter_map(|cell| {
                if cell.continuation {
                    None
                } else {
                    Some(cell.grapheme.as_deref().unwrap_or(" "))
                }
            })
            .collect()
    }

    pub(crate) fn has_painted_background(&self) -> bool {
        self.cells.iter().any(|cell| {
            cell.painted
                && cell.style.background.is_some()
                && cell.grapheme.is_none()
                && !cell.continuation
        })
    }

    pub(crate) fn style_at(&self, index: usize) -> Option<PhysicalStyle> {
        self.cell(index).map(|cell| cell.style)
    }
}
