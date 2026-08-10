//! Ratatui lowering for the remaining terminal-owned palette value.

use crate::terminal::ratatui::style;
use ratatui::style::Style;

pub(crate) fn input_border() -> Style {
    style(super::physical_style("input.border"))
}
