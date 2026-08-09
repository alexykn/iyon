use super::Size;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Rect {
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl Rect {
    pub(crate) const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

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

    pub(crate) const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub(crate) fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if left >= right || top >= bottom {
            return None;
        }
        Some(Self::new(left, top, right - left, bottom - top))
    }

    pub(crate) fn translate(self, point: super::Point) -> Self {
        Self::new(
            point.x.saturating_add(self.x),
            point.y.saturating_add(self.y),
            self.width,
            self.height,
        )
    }
}
