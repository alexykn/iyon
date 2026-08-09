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

pub(crate) fn physical_style(value: Style) -> PhysicalStyle {
    let foreground = value.fg.map(physical_color);
    let background = value.bg.map(physical_color);
    PhysicalStyle {
        foreground,
        background,
        bold: value.add_modifier.contains(Modifier::BOLD),
        dim: value.add_modifier.contains(Modifier::DIM),
        italic: value.add_modifier.contains(Modifier::ITALIC),
        underline: value.add_modifier.contains(Modifier::UNDERLINED),
        reversed: value.add_modifier.contains(Modifier::REVERSED),
    }
}

fn physical_color(value: Color) -> PhysicalColor {
    match value {
        Color::Reset => PhysicalColor::Default,
        Color::Indexed(value) => PhysicalColor::Indexed(value),
        Color::Rgb(r, g, b) => PhysicalColor::Rgb { r, g, b },
        Color::Black => PhysicalColor::Named(AnsiColor::Black),
        Color::Red => PhysicalColor::Named(AnsiColor::Red),
        Color::Green => PhysicalColor::Named(AnsiColor::Green),
        Color::Yellow => PhysicalColor::Named(AnsiColor::Yellow),
        Color::Blue => PhysicalColor::Named(AnsiColor::Blue),
        Color::Magenta => PhysicalColor::Named(AnsiColor::Magenta),
        Color::Cyan => PhysicalColor::Named(AnsiColor::Cyan),
        Color::Gray => PhysicalColor::Named(AnsiColor::Gray),
        Color::DarkGray => PhysicalColor::Named(AnsiColor::DarkGray),
        Color::LightRed => PhysicalColor::Named(AnsiColor::LightRed),
        Color::LightGreen => PhysicalColor::Named(AnsiColor::LightGreen),
        Color::LightYellow => PhysicalColor::Named(AnsiColor::LightYellow),
        Color::LightBlue => PhysicalColor::Named(AnsiColor::LightBlue),
        Color::LightMagenta => PhysicalColor::Named(AnsiColor::LightMagenta),
        Color::LightCyan => PhysicalColor::Named(AnsiColor::LightCyan),
        Color::White => PhysicalColor::Named(AnsiColor::White),
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
