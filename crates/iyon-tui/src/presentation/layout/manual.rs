//! Current manual semantic layout algorithm.
//!
//! This module owns width resolution, recursive View traversal, and composition
//! into backend-neutral physical surfaces. It intentionally has no terminal
//! backend dependency.

use unicode_width::UnicodeWidthStr;

use crate::{
    physical::{PhysicalCell, PhysicalStyle, Surface},
    presentation::{
        HorizontalAlign, IntoView, OverflowIndicator, TextSpan, VerticalAlign, View, WidthRule,
        ir::{ColumnView, ContainerNode, RowView, TextView, ViewKind},
    },
};

use super::{ViewCompiler, allocate_tracks};
use crate::presentation::paint::paint_border;

impl ViewCompiler {
    pub(crate) fn layout(&self, view: &View, max_width: u16, inherited: PhysicalStyle) -> Surface {
        let decoration = &view.decoration;
        let resolved = self
            .theme
            .resolve_text_style(inherited, &decoration.text_style);
        let border = u16::from(decoration.border.is_some());
        let border_pad = border.saturating_mul(2);
        let max_content = max_width.saturating_sub(border_pad);

        let left_pad = decoration.padding.left.min(max_content.saturating_sub(1));
        let right_pad = decoration
            .padding
            .right
            .min(max_content.saturating_sub(left_pad.saturating_add(1)));

        let horizontal = left_pad
            .saturating_add(right_pad)
            .saturating_add(border_pad);
        let inner_width = max_width.saturating_sub(horizontal);
        let core = self.layout_kind(&view.kind, view.width, inner_width, resolved);
        let requested_width = core.width().saturating_add(horizontal).min(u16::MAX);
        let width = match view.width {
            WidthRule::Fit => requested_width.min(max_width),
            WidthRule::Fill => max_width,
        };
        let height = core
            .height()
            .saturating_add(decoration.padding.top)
            .saturating_add(decoration.padding.bottom)
            .saturating_add(border.saturating_mul(2));
        let mut output = Surface::new(width, height);

        let child_x = border.saturating_add(left_pad);
        let child_y = border.saturating_add(decoration.padding.top);
        output.composite(&core, child_x, child_y);
        if let Some(color) = &decoration.surface_background {
            output.apply_surface_background(self.theme.resolve_color(color));
        }
        if let Some(border_spec) = &decoration.border {
            paint_border(&mut output, border_spec, &self.theme, resolved);
        }
        output
    }

    fn layout_kind(
        &self,
        kind: &ViewKind,
        width_rule: WidthRule,
        max_width: u16,
        inherited: PhysicalStyle,
    ) -> Surface {
        match kind {
            ViewKind::Text(text) => self.layout_text(width_rule, text, max_width, inherited),
            ViewKind::Column(column) => {
                self.layout_column(width_rule, column, max_width, inherited)
            }
            ViewKind::Row(row) => self.layout_row(width_rule, row, max_width, inherited),
            ViewKind::Container(ContainerNode { child }) => {
                self.layout(child, max_width, inherited)
            }
            // Spacer is vertical space with zero intrinsic cross-axis width.
            // The enclosing View shell applies Fill allocation uniformly.
            ViewKind::Spacer { rows } => Surface::new(0, *rows),
            ViewKind::ClampRows(clamp) => self.layout_clamp(clamp, max_width, inherited),
            ViewKind::ComponentSlot(_) => {
                unreachable!("component slot reached presentation layout without scene resolution")
            }
        }
    }

    fn layout_text(
        &self,
        width_rule: WidthRule,
        text: &TextView,
        max_width: u16,
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

    fn layout_column(
        &self,
        width_rule: WidthRule,
        column: &ColumnView,
        max_width: u16,
        inherited: PhysicalStyle,
    ) -> Surface {
        let children = column
            .children
            .iter()
            .map(|child| self.layout(child, max_width, inherited))
            .collect::<Vec<_>>();
        let content_width = children
            .iter()
            .map(|child| child.width())
            .max()
            .unwrap_or(0);
        let width = match width_rule {
            WidthRule::Fit => content_width.min(max_width),
            WidthRule::Fill => max_width,
        };
        let gap = usize::from(column.gap);
        let height = children
            .iter()
            .map(|child| usize::from(child.height()))
            .sum::<usize>()
            .saturating_add(gap.saturating_mul(children.len().saturating_sub(1)));
        let mut output = Surface::new(width, height.min(usize::from(u16::MAX)) as u16);
        let mut y = 0u16;
        for child in children {
            output.composite(&child, 0, y);
            y = y.saturating_add(child.height()).saturating_add(column.gap);
        }
        output
    }

    fn layout_row(
        &self,
        width_rule: WidthRule,
        row: &RowView,
        max_width: u16,
        inherited: PhysicalStyle,
    ) -> Surface {
        let allocation = allocate_tracks(row, max_width, self, inherited);
        let children = row
            .children
            .iter()
            .zip(allocation.tracks.iter().copied())
            .map(|(child, track)| {
                let surface = self.layout(&child.view, track, inherited);
                (track, surface)
            })
            .collect::<Vec<_>>();
        let content_height = children
            .iter()
            .map(|(_, child)| child.height())
            .max()
            .unwrap_or(0);
        let content_width = allocation
            .tracks
            .iter()
            .map(|track| usize::from(*track))
            .sum::<usize>()
            .saturating_add(
                usize::from(allocation.gap).saturating_mul(row.children.len().saturating_sub(1)),
            )
            .min(usize::from(u16::MAX)) as u16;
        let width = match width_rule {
            WidthRule::Fit => content_width.min(max_width),
            WidthRule::Fill => max_width,
        };
        let mut output = Surface::new(width, content_height);
        let mut x = 0u16;
        for (track, child) in children {
            let y = match row.vertical_align {
                VerticalAlign::Top => 0,
                VerticalAlign::Center => content_height.saturating_sub(child.height()) / 2,
                VerticalAlign::Bottom => content_height.saturating_sub(child.height()),
            };
            output.composite(&child, x, y);
            x = x.saturating_add(track).saturating_add(allocation.gap);
        }
        output
    }

    fn layout_clamp(
        &self,
        clamp: &crate::presentation::ir::ClampRowsView,
        max_width: u16,
        inherited: PhysicalStyle,
    ) -> Surface {
        let child = self.layout(&clamp.child, max_width, inherited);
        if child.height() <= clamp.max_rows {
            return child;
        }
        if clamp.max_rows == 0 {
            return Surface::new(child.width(), 0);
        }

        let mut output = Surface::new(child.width(), clamp.max_rows);
        output.physically_complete = child.physically_complete;
        for y in 0..clamp.max_rows {
            for x in 0..child.width() {
                *output.get_mut(x, y) = child.get(x, y).clone();
            }
        }
        let indicator = match &clamp.overflow {
            OverflowIndicator::None => None,
            OverflowIndicator::Ellipsis { style } => Some(("…".to_string(), style.clone())),
            OverflowIndicator::Footer { prefix, style } => Some((prefix.clone(), style.clone())),
        };
        if let Some((text, style)) = indicator {
            let indicator_view = View::styled_text(vec![TextSpan::styled(text, style)])
                .width(WidthRule::Fill)
                .no_wrap();
            let indicator_surface =
                self.layout(&indicator_view.into_view(), child.width(), inherited);
            let row = clamp.max_rows - 1;
            for x in 0..child.width() {
                *output.get_mut(x, row) = PhysicalCell::transparent();
                if x < indicator_surface.width() {
                    *output.get_mut(x, row) = indicator_surface.get(x, 0).clone();
                }
            }
        }
        output
    }
}
