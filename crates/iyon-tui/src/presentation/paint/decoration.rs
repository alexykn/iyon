//! Physical border and surface decoration painting.

use crate::{
    physical::{PhysicalStyle, Surface},
    presentation::{BorderSpec, BorderStyle},
};

use super::ThemeResolver;

pub(crate) fn paint_border(
    surface: &mut Surface,
    border: &BorderSpec,
    theme: &ThemeResolver,
    inherited: PhysicalStyle,
) {
    if surface.width() == 0 || surface.height() == 0 {
        return;
    }
    let style = border_style(border, theme, inherited);
    let (horizontal, vertical, corners) = match border.style {
        BorderStyle::Plain => ('─', '│', ('┌', '┐', '└', '┘')),
        BorderStyle::Rounded => ('─', '│', ('╭', '╮', '╰', '╯')),
        BorderStyle::Double => ('═', '║', ('╔', '╗', '╚', '╝')),
    };
    for x in 0..surface.width() {
        set_cell(surface, x, 0, horizontal.to_string(), style);
        if surface.height() > 1 {
            set_cell(
                surface,
                x,
                surface.height() - 1,
                horizontal.to_string(),
                style,
            );
        }
    }
    for y in 0..surface.height() {
        set_cell(surface, 0, y, vertical.to_string(), style);
        if surface.width() > 1 {
            set_cell(surface, surface.width() - 1, y, vertical.to_string(), style);
        }
    }
    set_cell(surface, 0, 0, corners.0.to_string(), style);
    if surface.width() > 1 {
        set_cell(
            surface,
            surface.width() - 1,
            0,
            corners.1.to_string(),
            style,
        );
    }
    if surface.height() > 1 {
        set_cell(
            surface,
            0,
            surface.height() - 1,
            corners.2.to_string(),
            style,
        );
        if surface.width() > 1 {
            set_cell(
                surface,
                surface.width() - 1,
                surface.height() - 1,
                corners.3.to_string(),
                style,
            );
        }
    }
}

fn border_style(
    border: &BorderSpec,
    theme: &ThemeResolver,
    inherited: PhysicalStyle,
) -> PhysicalStyle {
    let mut style = border
        .color
        .as_ref()
        .map(|color| PhysicalStyle {
            foreground: Some(theme.resolve_color(color)),
            ..PhysicalStyle::default()
        })
        .unwrap_or(inherited);
    // Text backgrounds belong only to descendant text cells. Border cells
    // retain the backing surface background established before border paint.
    style.background = None;
    style
}

fn set_cell(surface: &mut Surface, x: u16, y: u16, grapheme: String, mut style: PhysicalStyle) {
    if x >= surface.width() || y >= surface.height() {
        return;
    }
    style.background = surface.get(x, y).style.background;
    let cell = surface.get_mut(x, y);
    cell.grapheme = Some(grapheme);
    cell.style = style;
    cell.painted = true;
    cell.continuation = false;
}
