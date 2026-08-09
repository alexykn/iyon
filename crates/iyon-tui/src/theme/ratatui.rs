//! Ratatui compatibility wrappers over the shared backend-neutral palette.

use crate::terminal::ratatui::{color, style};
use ratatui::style::{Color, Style};

pub(crate) fn thinking() -> Style {
    style(super::physical_style("thinking"))
}

pub(crate) fn user_bubble_bg() -> Color {
    color(super::physical_user_bubble_bg())
}

pub(crate) fn muted() -> Style {
    style(super::physical_muted())
}

pub(crate) fn truncation_footer() -> Style {
    style(super::physical_style("truncation_footer"))
}

pub(crate) fn tool_finished() -> Style {
    style(super::physical_tool_finished())
}

pub(crate) fn tool_running() -> Style {
    style(super::physical_tool_running())
}

pub(crate) fn tool_error() -> Style {
    style(super::physical_tool_error())
}

pub(crate) fn input_border() -> Style {
    style(super::physical_style("input.border"))
}

pub(crate) fn markdown_header() -> Style {
    style(super::physical_markdown_header())
}

pub(crate) fn markdown_bold() -> Style {
    style(super::physical_markdown_bold())
}

pub(crate) fn markdown_italic() -> Style {
    style(super::physical_markdown_italic())
}

pub(crate) fn markdown_code() -> Style {
    style(super::physical_markdown_code())
}

pub(crate) fn markdown_list() -> Style {
    style(super::physical_markdown_list())
}
