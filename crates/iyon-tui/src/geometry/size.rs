#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Size {
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl Size {
    pub(crate) const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }
}
