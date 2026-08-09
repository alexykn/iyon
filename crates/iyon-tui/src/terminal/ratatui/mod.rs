//! Ratatui-only lowering and drawing adapter.

mod render;
mod row;
mod style;

#[cfg(test)]
mod tests;

pub(crate) use render::render_view;
pub(crate) use row::{row_to_line, rows_to_lines};
pub(crate) use style::physical_style;
