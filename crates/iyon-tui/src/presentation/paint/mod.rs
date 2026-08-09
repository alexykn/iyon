//! Backend-neutral semantic style and decoration resolution.

mod decoration;
mod legacy;
mod legacy_text;
mod theme;
mod view;

pub(crate) use decoration::paint_border;
pub(crate) use legacy_text::{CompiledTextRow, row_from_graphemes, row_from_string};
pub(crate) use theme::ThemeResolver;
pub(crate) use view::ViewPainter;
