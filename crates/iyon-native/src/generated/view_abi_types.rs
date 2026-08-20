// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 68e52f9913c6c1252f2a061ff4f942c1b32aac35f32bced41e8f9bdc5b2bacb9
// generator_blake3 = 24d34b5e76bb7302928f251bbf11d78e62dfba0dee9cefe44e46082a1aeedc18
//! Canonical pointer-free ABI types and constants.

pub const ABI_NAME: &str = "iyon_tui_view";
pub const ABI_VERSION: u32 = 1;
pub const SEMANTIC_SCHEMA_VERSION: u32 = 1;
pub const MINIMUM_BUN: &str = "1.4.0";
pub const QUALIFIED_BUN: &str = "1.4.0";
pub const RESULT_ERROR_BIT: u32 = 0x8000_0000;

pub type ViewRefResult = u32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AxisChildInputV1 {
    pub track_word: u32,
    pub child_ref: u32,
}

static_assertions::const_assert_eq!(::core::mem::size_of::<AxisChildInputV1>(), 8);
static_assertions::const_assert_eq!(::core::mem::align_of::<AxisChildInputV1>(), 4);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NativeViewAbiHeader {
    pub magic: u32,
    pub abi_version: u32,
    pub semantic_version: u32,
    pub alive: u32,
}

static_assertions::const_assert_eq!(::core::mem::size_of::<NativeViewAbiHeader>(), 16);

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WrapMode {
    WordThenGrapheme = 1,
    Grapheme = 2,
    NoWrap = 3,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HorizontalAlign {
    Start = 1,
    Center = 2,
    End = 3,
}
