//! Ratatui-only lowering and drawing adapter.

mod render;
mod row;
mod style;

#[cfg(test)]
mod tests;

pub(crate) use render::{render_history_overlay, render_surface};
pub(crate) use row::rows_to_lines;
