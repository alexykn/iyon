// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = d678d329a5e75554bc9572deb3a4b0dbd95c505cbfc6b1c2de7635483ac81914
// generator_blake3 = 6a3096554d5af17ad3d1aee961024cf2303a623e5ec4a1ecf60275343341dc91
#![allow(dead_code)]

//! Canonical pointer-free ABI types and constants.

pub const SCHEMA_BLAKE3: &str = "d678d329a5e75554bc9572deb3a4b0dbd95c505cbfc6b1c2de7635483ac81914";
pub const GENERATOR_BLAKE3: &str =
    "6a3096554d5af17ad3d1aee961024cf2303a623e5ec4a1ecf60275343341dc91";

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

static_assertions::const_assert_eq!(WrapMode::WordThenGrapheme as u32, 1);
static_assertions::const_assert_eq!(WrapMode::Grapheme as u32, 2);
static_assertions::const_assert_eq!(WrapMode::NoWrap as u32, 3);

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HorizontalAlign {
    Start = 1,
    Center = 2,
    End = 3,
}

static_assertions::const_assert_eq!(HorizontalAlign::Start as u32, 1);
static_assertions::const_assert_eq!(HorizontalAlign::Center as u32, 2);
static_assertions::const_assert_eq!(HorizontalAlign::End as u32, 3);
