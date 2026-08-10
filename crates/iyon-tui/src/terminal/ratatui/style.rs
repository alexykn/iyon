//! Ratatui lowering for backend-neutral physical styles.

use crate::physical::{AnsiColor, PhysicalColor, PhysicalStyle};
use ratatui::style::{Color, Modifier, Style};

pub(crate) fn color(value: PhysicalColor) -> Color {
    match value {
        PhysicalColor::Default => Color::Reset,
        PhysicalColor::Named(value) => named_color(value),
        PhysicalColor::Indexed(value) => Color::Indexed(value),
        PhysicalColor::Rgb { r, g, b } => Color::Rgb(r, g, b),
    }
}

fn named_color(value: AnsiColor) -> Color {
    match value {
        AnsiColor::Black => Color::Black,
        AnsiColor::Red => Color::Red,
        AnsiColor::Green => Color::Green,
        AnsiColor::Yellow => Color::Yellow,
        AnsiColor::Blue => Color::Blue,
        AnsiColor::Magenta => Color::Magenta,
        AnsiColor::Cyan => Color::Cyan,
        AnsiColor::Gray => Color::Gray,
        AnsiColor::DarkGray => Color::DarkGray,
        AnsiColor::LightRed => Color::LightRed,
        AnsiColor::LightGreen => Color::LightGreen,
        AnsiColor::LightYellow => Color::LightYellow,
        AnsiColor::LightBlue => Color::LightBlue,
        AnsiColor::LightMagenta => Color::LightMagenta,
        AnsiColor::LightCyan => Color::LightCyan,
        AnsiColor::White => Color::White,
    }
}

pub(crate) fn style(value: PhysicalStyle) -> Style {
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
