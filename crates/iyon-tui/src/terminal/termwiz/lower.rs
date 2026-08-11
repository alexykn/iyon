use termwiz::{
    cell::{CellAttributes, Intensity, Underline},
    color::{AnsiColor, ColorAttribute, RgbColor},
    surface::{Change, Position, Surface},
};

use crate::{
    physical::{
        AnsiColor as IyonAnsiColor, PhysicalCell, PhysicalColor, PhysicalRow, PhysicalStyle,
    },
    scene::PreparedSceneFrame,
};

pub(crate) fn desired_surface(frame: &PreparedSceneFrame) -> Surface {
    let mut physical = frame.surface.clone();
    if let Some(overlay) = &frame.history_overlay {
        apply_overlay(&mut physical, overlay);
    }

    let mut desired = Surface::new(
        usize::from(physical.width()),
        usize::from(physical.height()),
    );
    desired.add_changes(surface_changes(&physical));
    if physical.width() > 0 && physical.height() > 0 {
        desired.add_change(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(usize::from(physical.height().saturating_sub(1))),
        });
        desired.add_change(Change::CursorVisibility(
            termwiz::surface::CursorVisibility::Hidden,
        ));
    }
    desired
}

pub(crate) fn surface_changes(surface: &crate::physical::Surface) -> Vec<Change> {
    let mut changes = Vec::new();
    for y in 0..surface.height() {
        let row = PhysicalRow::from_cells(
            (0..surface.width())
                .map(|x| surface.get(x, y).clone())
                .collect(),
        );
        changes.extend(row_changes(&row, usize::from(y), true));
    }
    changes
}

pub(crate) fn row_changes(row: &PhysicalRow, y: usize, clear_tail: bool) -> Vec<Change> {
    let last_written = row.cells().iter().rposition(|cell| cell.painted);
    let mut changes = vec![Change::CursorPosition {
        x: Position::Absolute(0),
        y: Position::Absolute(y),
    }];

    if clear_tail {
        changes.push(Change::AllAttributes(CellAttributes::default()));
        changes.push(Change::ClearToEndOfLine(ColorAttribute::Default));
    }

    let Some(last_written) = last_written else {
        return changes;
    };

    let mut active_style: Option<CellAttributes> = None;
    let mut text = String::new();
    for cell in &row.cells()[..=last_written] {
        if cell.continuation {
            continue;
        }

        let style = if cell.painted {
            physical_style(cell.style)
        } else {
            CellAttributes::default()
        };
        let grapheme = if cell.painted {
            cell.grapheme.as_deref().unwrap_or(" ")
        } else {
            " "
        };

        if active_style.as_ref() != Some(&style) {
            flush_text(&mut changes, &mut text, active_style.take());
            active_style = Some(style);
        }
        text.push_str(grapheme);
    }
    flush_text(&mut changes, &mut text, active_style);
    changes
}

fn flush_text(changes: &mut Vec<Change>, text: &mut String, style: Option<CellAttributes>) {
    let Some(style) = style else {
        return;
    };
    changes.push(Change::AllAttributes(style));
    if !text.is_empty() {
        changes.push(Change::Text(std::mem::take(text)));
    }
}

pub(crate) fn physical_style(value: PhysicalStyle) -> CellAttributes {
    let mut attributes = CellAttributes::default();
    attributes.set_foreground(
        value
            .foreground
            .map(color)
            .unwrap_or(ColorAttribute::Default),
    );
    attributes.set_background(
        value
            .background
            .map(color)
            .unwrap_or(ColorAttribute::Default),
    );
    // Termwiz has one intensity field. Iyon's canonical styles do not rely on
    // simultaneous bold and dim; bold wins deterministically if both appear.
    attributes.set_intensity(if value.bold {
        Intensity::Bold
    } else if value.dim {
        Intensity::Half
    } else {
        Intensity::Normal
    });
    attributes.set_italic(value.italic);
    attributes.set_underline(if value.underline {
        Underline::Single
    } else {
        Underline::None
    });
    attributes.set_reverse(value.reversed);
    attributes
}

fn color(value: PhysicalColor) -> ColorAttribute {
    match value {
        PhysicalColor::Default => ColorAttribute::Default,
        PhysicalColor::Named(value) => ColorAttribute::PaletteIndex(ansi_color(value) as u8),
        PhysicalColor::Indexed(value) => ColorAttribute::PaletteIndex(value),
        PhysicalColor::Rgb { r, g, b } => {
            ColorAttribute::TrueColorWithDefaultFallback(RgbColor::new_8bpc(r, g, b).into())
        }
    }
}

fn ansi_color(value: IyonAnsiColor) -> AnsiColor {
    match value {
        IyonAnsiColor::Black => AnsiColor::Black,
        IyonAnsiColor::Red => AnsiColor::Maroon,
        IyonAnsiColor::Green => AnsiColor::Green,
        IyonAnsiColor::Yellow => AnsiColor::Olive,
        IyonAnsiColor::Blue => AnsiColor::Navy,
        IyonAnsiColor::Magenta => AnsiColor::Purple,
        IyonAnsiColor::Cyan => AnsiColor::Teal,
        IyonAnsiColor::Gray => AnsiColor::Silver,
        IyonAnsiColor::DarkGray => AnsiColor::Grey,
        IyonAnsiColor::LightRed => AnsiColor::Red,
        IyonAnsiColor::LightGreen => AnsiColor::Lime,
        IyonAnsiColor::LightYellow => AnsiColor::Yellow,
        IyonAnsiColor::LightBlue => AnsiColor::Blue,
        IyonAnsiColor::LightMagenta => AnsiColor::Fuchsia,
        IyonAnsiColor::LightCyan => AnsiColor::Aqua,
        IyonAnsiColor::White => AnsiColor::White,
    }
}

fn apply_overlay(
    surface: &mut crate::physical::Surface,
    overlay: &crate::history::HistoryPhysicalOverlay,
) {
    if surface.width() == 0 {
        return;
    }
    for (row_index, row) in overlay.rows.iter().enumerate() {
        let y = usize::from(overlay.row).saturating_add(row_index);
        if y >= usize::from(surface.height()) {
            continue;
        }
        let Some(last_written) = row.cells().iter().rposition(|cell| cell.painted) else {
            continue;
        };
        for x in 0..=last_written.min(usize::from(surface.width()).saturating_sub(1)) {
            let cell = row.cell(x).cloned().unwrap_or_else(|| PhysicalCell {
                grapheme: None,
                style: PhysicalStyle::default(),
                painted: true,
                continuation: false,
            });
            *surface.get_mut(x as u16, y as u16) = if cell.painted {
                cell
            } else {
                PhysicalCell {
                    grapheme: None,
                    style: PhysicalStyle::default(),
                    painted: true,
                    continuation: false,
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_style_maps_colors_and_normalizes_intensity() {
        let attributes = physical_style(PhysicalStyle {
            foreground: Some(PhysicalColor::Indexed(4)),
            background: Some(PhysicalColor::Rgb { r: 1, g: 2, b: 3 }),
            bold: true,
            dim: true,
            italic: true,
            underline: true,
            reversed: true,
        });
        assert_eq!(attributes.foreground(), ColorAttribute::PaletteIndex(4));
        assert_eq!(
            attributes.background(),
            ColorAttribute::TrueColorWithDefaultFallback(RgbColor::new_8bpc(1, 2, 3).into())
        );
        assert_eq!(attributes.intensity(), Intensity::Bold);
        assert!(attributes.italic());
        assert_eq!(attributes.underline(), Underline::Single);
        assert!(attributes.reverse());

        assert_eq!(
            physical_style(PhysicalStyle {
                dim: true,
                ..PhysicalStyle::default()
            })
            .intensity(),
            Intensity::Half
        );
    }

    #[test]
    fn wide_continuations_are_not_emitted_as_separate_text() {
        let row = PhysicalRow::from_cells(vec![
            PhysicalCell {
                grapheme: Some("界".to_string()),
                style: PhysicalStyle::default(),
                painted: true,
                continuation: false,
            },
            PhysicalCell {
                grapheme: None,
                style: PhysicalStyle::default(),
                painted: true,
                continuation: true,
            },
            PhysicalCell {
                grapheme: Some("!".to_string()),
                style: PhysicalStyle::default(),
                painted: true,
                continuation: false,
            },
        ]);
        let mut surface = Surface::new(4, 1);
        surface.add_changes(row_changes(&row, 0, true));
        assert_eq!(surface.screen_chars_to_string(), "界! \n");
    }
}
