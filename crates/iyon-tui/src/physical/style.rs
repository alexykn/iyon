//! Backend-neutral resolved colors and cell styles.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PhysicalColor {
    Default,
    Indexed(u8),
    Rgb { r: u8, g: u8, b: u8 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct PhysicalStyle {
    pub(crate) foreground: Option<PhysicalColor>,
    pub(crate) background: Option<PhysicalColor>,
    pub(crate) bold: bool,
    pub(crate) dim: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) reversed: bool,
}
