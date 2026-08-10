//! Physical border and surface decoration painting.

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    physical::{PhysicalStyle, Surface},
    presentation::BorderSpec,
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
    let edges = border.edges;
    let glyphs = &border.glyphs;
    let last_x = surface.width().saturating_sub(1);
    let last_y = surface.height().saturating_sub(1);

    if edges.top {
        for x in 0..surface.width() {
            set_cell(surface, x, 0, glyphs.top.clone(), style);
        }
    }
    if edges.bottom {
        for x in 0..surface.width() {
            set_cell(surface, x, last_y, glyphs.bottom.clone(), style);
        }
    }
    if edges.left {
        for y in 0..surface.height() {
            set_cell(surface, 0, y, glyphs.left.clone(), style);
        }
    }
    if edges.right {
        for y in 0..surface.height() {
            set_cell(surface, last_x, y, glyphs.right.clone(), style);
        }
    }

    if edges.top && edges.left {
        set_cell(surface, 0, 0, glyphs.top_left.clone(), style);
    }

    if edges.top {
        if let Some(label) = &border.top_label {
            for (x, grapheme) in label.graphemes(true).enumerate() {
                if x >= usize::from(surface.width()) {
                    break;
                }
                set_cell(surface, x as u16, 0, grapheme.to_owned(), style);
            }
        }
    }
    if edges.top && edges.right {
        set_cell(surface, last_x, 0, glyphs.top_right.clone(), style);
    }
    if edges.bottom && edges.left {
        set_cell(surface, 0, last_y, glyphs.bottom_left.clone(), style);
    }
    if edges.bottom && edges.right {
        set_cell(surface, last_x, last_y, glyphs.bottom_right.clone(), style);
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
