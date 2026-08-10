//! Private presentation boundaries for semantic construction and lowering.
//!
//! `api` is the curated semantic construction facade. `ir` is the private
//! retained semantic tree. `layout` owns ordinary semantic compilation into
//! physical rows, `paint` resolves styles and decoration, and `wrap` owns
//! Unicode wrapping. Generic stream provenance is a sibling `stream` subsystem.

pub(crate) mod api;
pub(crate) mod ir;
pub(crate) mod layout;
pub(crate) mod paint;
pub(crate) mod wrap;

#[allow(unused_imports)]
pub(crate) use api::{
    BorderEdges, BorderGlyphs, BorderSpec, BorderStyle, ColorSpec, Horizontal, HorizontalAlign,
    Insets, IntoView, OverflowIndicator, StyleSpec, Text, TextAttribute, TextAttributeSpec,
    TextSpan, ThemeKey, Vertical, VerticalAlign, View, WrapMode,
};

// Retained IR types remain private implementation details of the semantic
// layout engine.
pub(crate) use ir::{Decoration, RowChild, WidthRule};
