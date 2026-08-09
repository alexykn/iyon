//! Semantic construction facade.
//!
//! Consumers construct the canonical owned View IR through these types
//! without depending on retained structural implementation details.

mod composition;
pub(super) mod style;
pub(super) mod text;
mod view;

pub(crate) use super::ir::View;
pub(crate) use composition::{Horizontal, Vertical};
pub(crate) use style::{
    BorderSpec, BorderStyle, ColorSpec, Insets, OverflowIndicator, StyleSpec, TextAttribute,
    TextAttributeSpec, ThemeKey, VerticalAlign,
};
pub(crate) use text::{HorizontalAlign, Text, TextSpan, WrapMode};
pub(crate) use view::IntoView;
