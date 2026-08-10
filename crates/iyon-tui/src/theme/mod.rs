//! Single source of truth for the application's semantic palette.
//!
//! The palette is stored as backend-neutral physical values.

use crate::physical::{AnsiColor, PhysicalColor};

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
        "tool.error" | "text.error" => PhysicalColor::Named(AnsiColor::Red),
        "text.warning" => PhysicalColor::Named(AnsiColor::Yellow),
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
