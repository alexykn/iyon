//! Compatibility text painter for the established physical text behavior.
//!
//! Structural layout is owned by `ManualLayoutEngine`. This module only turns
//! an already-allocated text leaf into backend-neutral physical cells.

use unicode_width::UnicodeWidthStr;

use crate::{
    physical::{PhysicalCell, PhysicalStyle, Surface},
    presentation::{HorizontalAlign, WidthRule, ir::TextView},
};

use crate::presentation::layout::ViewCompiler;

impl ViewCompiler {
    pub(crate) fn paint_text(
        &self,
        text: &TextView,
        max_width: u16,
        width_rule: WidthRule,
        inherited: PhysicalStyle,
    ) -> Surface {
        let (width, rows) =
            self.compile_text_with_metadata(text, max_width, width_rule, inherited, true);
        let all_fit = rows.iter().all(|row| row.fits);
        let mut surface = Surface::new(width, rows.len().max(1) as u16);
        surface.physically_complete = all_fit;
        for (y, row) in rows.into_iter().enumerate() {
            let line_width = row.width;
            let offset = match text.align {
                HorizontalAlign::Start => 0,
                HorizontalAlign::Center => usize::from(width).saturating_sub(line_width) / 2,
                HorizontalAlign::End => usize::from(width).saturating_sub(line_width),
            };
            for (index, source) in row.row.cells().iter().enumerate() {
                let x = offset.saturating_add(index);
                if x >= usize::from(width) {
                    break;
                }
                if source.painted {
                    if !source.continuation
                        && source.grapheme.as_deref().is_some_and(|grapheme| {
                            UnicodeWidthStr::width(grapheme) > usize::from(width).saturating_sub(x)
                        })
                    {
                        continue;
                    }
                    *surface.get_mut(x as u16, y as u16) = source.clone();
                }
            }
            if let Some(column) = row.cursor_column {
                let x = offset.saturating_add(column);
                if x < usize::from(width) {
                    let marker_style = row
                        .row
                        .cell(column)
                        .or_else(|| row.row.cells().last())
                        .map_or(inherited, |cell| cell.style);
                    let cell = surface.get_mut(x as u16, y as u16);
                    if !cell.painted {
                        *cell = PhysicalCell {
                            grapheme: Some(" ".to_owned()),
                            style: marker_style,
                            painted: true,
                            continuation: false,
                        };
                    }
                    cell.style.reversed = true;
                }
            }
        }
        surface
    }
}
