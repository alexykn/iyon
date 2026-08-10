//! Mutable backend-neutral physical composition surface.

use crate::geometry::Size;

use super::{PhysicalColor, PhysicalStyle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PhysicalCell {
    pub(crate) grapheme: Option<String>,
    pub(crate) style: PhysicalStyle,
    pub(crate) painted: bool,
    pub(crate) continuation: bool,
}

impl PhysicalCell {
    pub(crate) fn transparent() -> Self {
        Self {
            grapheme: None,
            style: PhysicalStyle::default(),
            painted: false,
            continuation: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn blank(style: PhysicalStyle) -> Self {
        Self {
            grapheme: None,
            style,
            painted: true,
            continuation: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Surface {
    pub(crate) size: Size,
    pub(crate) cells: Vec<PhysicalCell>,
    pub(crate) physically_complete: bool,
}

impl Surface {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            size: Size { width, height },
            cells: vec![PhysicalCell::transparent(); usize::from(width) * usize::from(height)],
            physically_complete: true,
        }
    }

    pub(crate) fn width(&self) -> u16 {
        self.size.width
    }

    pub(crate) fn height(&self) -> u16 {
        self.size.height
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width()) + usize::from(x)
    }

    pub(crate) fn get(&self, x: u16, y: u16) -> &PhysicalCell {
        &self.cells[self.index(x, y)]
    }

    pub(crate) fn get_mut(&mut self, x: u16, y: u16) -> &mut PhysicalCell {
        let index = self.index(x, y);
        &mut self.cells[index]
    }

    pub(crate) fn apply_surface_background(&mut self, color: PhysicalColor) {
        for cell in &mut self.cells {
            if !cell.painted {
                cell.style = PhysicalStyle {
                    background: Some(color),
                    ..PhysicalStyle::default()
                };
                cell.grapheme = None;
                cell.continuation = false;
                cell.painted = true;
            } else if cell.style.background.is_none() {
                cell.style.background = Some(color);
            }
        }
    }

    pub(crate) fn composite(&mut self, child: &Self, x: u16, y: u16) {
        if !child.physically_complete {
            self.physically_complete = false;
        }
        for child_y in 0..child.height() {
            let target_y = y.saturating_add(child_y);
            if target_y >= self.height() {
                continue;
            }
            for child_x in 0..child.width() {
                let target_x = x.saturating_add(child_x);
                if target_x >= self.width() {
                    continue;
                }
                let source = child.get(child_x, child_y);
                if source.painted {
                    *self.get_mut(target_x, target_y) = source.clone();
                }
            }
        }
    }
}
