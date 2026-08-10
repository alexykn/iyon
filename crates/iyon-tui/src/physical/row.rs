//! Immutable final physical rows.

use super::PhysicalCell;
#[cfg(test)]
use super::PhysicalStyle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhysicalRow {
    cells: Vec<PhysicalCell>,
}

impl PhysicalRow {
    pub(crate) fn from_cells(cells: Vec<PhysicalCell>) -> Self {
        Self { cells }
    }

    pub(crate) fn placed(&self, width: u16, left: u16) -> Self {
        let mut cells = vec![PhysicalCell::transparent(); usize::from(width)];
        let start = usize::from(left).min(cells.len());
        let copy_len = self.width().min(cells.len().saturating_sub(start));
        cells[start..start + copy_len].clone_from_slice(&self.cells[..copy_len]);
        Self::from_cells(cells)
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

    #[cfg(test)]
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

    #[cfg(test)]
    pub(crate) fn style_at(&self, index: usize) -> Option<PhysicalStyle> {
        self.cell(index).map(|cell| cell.style)
    }
}
