use super::Size;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AxisConstraint {
    Definite(u16),
    Unbounded,
}

impl AxisConstraint {
    pub(crate) const fn definite(self) -> Option<u16> {
        match self {
            Self::Definite(value) => Some(value),
            Self::Unbounded => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayoutConstraints {
    pub(crate) width: AxisConstraint,
    pub(crate) height: AxisConstraint,
}

impl LayoutConstraints {
    pub(crate) const fn width_only(width: u16) -> Self {
        Self {
            width: AxisConstraint::Definite(width),
            height: AxisConstraint::Unbounded,
        }
    }

    pub(crate) const fn bounded(size: Size) -> Self {
        Self {
            width: AxisConstraint::Definite(size.width),
            height: AxisConstraint::Definite(size.height),
        }
    }
}
