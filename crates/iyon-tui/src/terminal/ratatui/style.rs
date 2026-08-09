//! Ratatui lowering for backend-neutral physical styles.

use crate::physical::{PhysicalColor, PhysicalStyle};
use ratatui::style::{Color, Modifier, Style};

pub(crate) fn color(value: PhysicalColor) -> Color {
    match value {
        PhysicalColor::Default => Color::Reset,
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
        Color::Black => PhysicalColor::Indexed(0),
        Color::Red => PhysicalColor::Indexed(1),
        Color::Green => PhysicalColor::Indexed(2),
        Color::Yellow => PhysicalColor::Indexed(3),
        Color::Blue => PhysicalColor::Indexed(4),
        Color::Magenta => PhysicalColor::Indexed(5),
        Color::Cyan => PhysicalColor::Indexed(6),
        Color::Gray => PhysicalColor::Indexed(7),
        Color::DarkGray => PhysicalColor::Indexed(8),
        Color::LightRed => PhysicalColor::Indexed(9),
        Color::LightGreen => PhysicalColor::Indexed(10),
        Color::LightYellow => PhysicalColor::Indexed(11),
        Color::LightBlue => PhysicalColor::Indexed(12),
        Color::LightMagenta => PhysicalColor::Indexed(13),
        Color::LightCyan => PhysicalColor::Indexed(14),
        Color::White => PhysicalColor::Indexed(15),
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
