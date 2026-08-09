//! Single source of truth for the application's semantic palette.
//!
//! The palette is stored as backend-neutral physical values. The `ratatui`
//! module exposes compatibility constructors for legacy application code.

mod ratatui;

use crate::physical::{PhysicalColor, PhysicalStyle};

pub(crate) use self::ratatui::{
    input_border, markdown_bold, markdown_code, markdown_header, markdown_italic, markdown_list,
    muted, thinking, truncation_footer,
};

pub(crate) fn physical_color(key: &str) -> PhysicalColor {
    match key {
        "surface.user" => PhysicalColor::Rgb {
            r: 45,
            g: 55,
            b: 72,
        },
        "text.muted" | "surface.default" => PhysicalColor::Rgb {
            r: 113,
            g: 128,
            b: 150,
        },
        "tool.running" => PhysicalColor::Rgb {
            r: 160,
            g: 174,
            b: 192,
        },
        "tool.finished" => PhysicalColor::Rgb {
            r: 104,
            g: 211,
            b: 145,
        },
        "tool.error" | "text.error" => PhysicalColor::Indexed(1),
        "text.warning" => PhysicalColor::Indexed(3),
        "markdown.header" => PhysicalColor::Rgb {
            r: 255,
            g: 196,
            b: 87,
        },
        "markdown.bold" => PhysicalColor::Rgb {
            r: 230,
            g: 230,
            b: 235,
        },
        "markdown.italic" => PhysicalColor::Rgb {
            r: 150,
            g: 180,
            b: 220,
        },
        "markdown.code" => PhysicalColor::Rgb {
            r: 120,
            g: 200,
            b: 210,
        },
        "markdown.list" => PhysicalColor::Rgb {
            r: 104,
            g: 211,
            b: 145,
        },
        "truncation_footer" => PhysicalColor::Rgb {
            r: 120,
            g: 122,
            b: 132,
        },
        "input.border" => PhysicalColor::Rgb {
            r: 173,
            g: 216,
            b: 230,
        },
        _ => PhysicalColor::Default,
    }
}

pub(crate) fn physical_style(key: &str) -> PhysicalStyle {
    let foreground = Some(physical_color(key));
    let mut style = PhysicalStyle {
        foreground,
        ..PhysicalStyle::default()
    };
    match key {
        "thinking" => {
            style.foreground = Some(physical_color("text.muted"));
            style.italic = true;
        }
        "truncation_footer" => {
            style.italic = true;
            style.dim = true;
        }
        "markdown.bold" => style.bold = true,
        "markdown.italic" => style.italic = true,
        _ => {}
    }
    style
}

pub(crate) fn physical_user_bubble_bg() -> PhysicalColor {
    physical_color("surface.user")
}

pub(crate) fn physical_muted() -> PhysicalStyle {
    physical_style("text.muted")
}

pub(crate) fn physical_tool_running() -> PhysicalStyle {
    physical_style("tool.running")
}

pub(crate) fn physical_tool_finished() -> PhysicalStyle {
    physical_style("tool.finished")
}

pub(crate) fn physical_tool_error() -> PhysicalStyle {
    physical_style("tool.error")
}

pub(crate) fn physical_markdown_header() -> PhysicalStyle {
    physical_style("markdown.header")
}

pub(crate) fn physical_markdown_bold() -> PhysicalStyle {
    physical_style("markdown.bold")
}

pub(crate) fn physical_markdown_italic() -> PhysicalStyle {
    physical_style("markdown.italic")
}

pub(crate) fn physical_markdown_code() -> PhysicalStyle {
    physical_style("markdown.code")
}

pub(crate) fn physical_markdown_list() -> PhysicalStyle {
    physical_style("markdown.list")
}
