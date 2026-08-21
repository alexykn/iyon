// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 0e958fc44a24679d0503ae6606bcf1529e0b7caf5eed48097ddbe2ade7d60ef6
// generator_blake3 = 20435cb0e211e543dd671e6c86669cf3f205c8e77c5070f47f4d181a4a9d3c71
#![allow(dead_code)]

//! Canonical pointer-free ABI types and constants.

pub const SCHEMA_BLAKE3: &str = "0e958fc44a24679d0503ae6606bcf1529e0b7caf5eed48097ddbe2ade7d60ef6";
pub const GENERATOR_BLAKE3: &str =
    "20435cb0e211e543dd671e6c86669cf3f205c8e77c5070f47f4d181a4a9d3c71";

pub const ABI_NAME: &str = "iyon_tui_view";
pub const ABI_VERSION: u32 = 1;
pub const SEMANTIC_SCHEMA_VERSION: u32 = 1;
pub const MINIMUM_BUN: &str = "1.4.0";
pub const QUALIFIED_BUN: &str = "1.4.0";
pub const RESULT_ERROR_BIT: u32 = 0x8000_0000;

pub type ViewRefResult = u32;
pub type StyleRefResult = u32;
pub type StyleAtomRefResult = u32;

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
