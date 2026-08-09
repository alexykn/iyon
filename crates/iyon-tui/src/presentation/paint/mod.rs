//! Backend-neutral semantic style and decoration resolution.

mod decoration;
mod theme;

pub(crate) use decoration::paint_border;
pub(crate) use theme::ThemeResolver;
