//! Ratatui compatibility wrappers over the shared backend-neutral palette.

use crate::physical::{PhysicalColor, PhysicalStyle};
use ratatui::style::{Color, Modifier, Style};

fn color(value: PhysicalColor) -> Color {
    match value {
        PhysicalColor::Default => Color::Reset,
        PhysicalColor::Indexed(value) => Color::Indexed(value),
        PhysicalColor::Rgb { r, g, b } => Color::Rgb(r, g, b),
    }
}

fn style(value: PhysicalStyle) -> Style {
    let mut output = Style {
        fg: value.foreground.map(color),
        bg: value.background.map(color),
        ..Style::default()
    };
    let mut modifiers = Modifier::empty();
    if value.bold {
        modifiers |= Modifier::BOLD;
    }
    if value.dim {
        modifiers |= Modifier::DIM;
    }
    if value.italic {
        modifiers |= Modifier::ITALIC;
    }
    if value.underline {
        modifiers |= Modifier::UNDERLINED;
    }
    if value.reversed {
        modifiers |= Modifier::REVERSED;
    }
    output.add_modifier = modifiers;
    output
}

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
    // Preserve the legacy named Ratatui color for compatibility callers.
    Style::default().fg(Color::Red)
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
