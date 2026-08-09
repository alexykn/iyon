//! Backend-neutral physical geometry used by presentation and terminal adapters.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Point {
    pub(crate) x: u16,
    pub(crate) y: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Size {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Rect {
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl Rect {
    pub(crate) const fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    pub(crate) const fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }

    pub(crate) const fn size(self) -> Size {
        Size {
            width: self.width,
            height: self.height,
        }
    }
}
