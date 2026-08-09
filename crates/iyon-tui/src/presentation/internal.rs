//! INTERNAL PRESENTATION MECHANICS.
//!
//! This module is the future library boundary's terminal adapter. It resolves
//! semantic views once the actual width is known, composes them on a transparent
//! cell surface, and only then lowers them to final physical `Line`s.

use std::borrow::Cow;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    presentation::stream::{ProjectedText, StreamRange},
    presentation::{
        api::{
            IntoView,
            style::{
                BorderSpec, BorderStyle, ColorSpec, OverflowIndicator, StyleSpec, VerticalAlign,
            },
            text::{HorizontalAlign, TextSpan},
        },
        ir::{ColumnView, ContainerNode, RowView, TextView, TrackSize, View, ViewKind, WidthRule},
    },
    theme,
};

/// INTERNAL PRESENTATION MECHANICS.
///
/// A transparent child cell and a deliberately painted blank are distinct until
/// composition is complete. This is what lets a parent background flow through
/// transparent row tails and gaps without losing styled blank padding.
#[derive(Clone, Debug)]
struct Surface {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
    physically_complete: bool,
}

#[derive(Clone, Debug)]
struct Cell {
    grapheme: Option<String>,
    style: Style,
    painted: bool,
    continuation: bool,
}

impl Cell {
    fn transparent() -> Self {
        Self {
            grapheme: None,
            style: Style::default(),
            painted: false,
            continuation: false,
        }
    }
}

impl Surface {
    fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::transparent(); usize::from(width) * usize::from(height)],
            physically_complete: true,
        }
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width) + usize::from(x)
    }

    fn get(&self, x: u16, y: u16) -> &Cell {
        &self.cells[self.index(x, y)]
    }

    fn get_mut(&mut self, x: u16, y: u16) -> &mut Cell {
        let index = self.index(x, y);
        &mut self.cells[index]
    }

    fn apply_surface_background(&mut self, color: Color) {
        for cell in &mut self.cells {
            if !cell.painted {
                cell.style = Style::default().bg(color);
                cell.grapheme = None;
                cell.continuation = false;
                cell.painted = true;
            } else if cell.style.bg.is_none() {
                cell.style.bg = Some(color);
            }
        }
    }

    fn composite(&mut self, child: &Surface, x: u16, y: u16) {
        if !child.physically_complete {
            self.physically_complete = false;
        }
        for child_y in 0..child.height {
            let target_y = y.saturating_add(child_y);
            if target_y >= self.height {
                continue;
            }
            for child_x in 0..child.width {
                let target_x = x.saturating_add(child_x);
                if target_x >= self.width {
                    continue;
                }
                let source = child.get(child_x, child_y);
                if source.painted {
                    *self.get_mut(target_x, target_y) = source.clone();
                }
            }
        }
    }
}

/// INTERNAL PRESENTATION MECHANICS. A fully width-resolved semantic result.
/// Its rows are final physical rows and must not pass through legacy wrapping.
#[derive(Clone, Debug)]
pub(crate) struct LayoutBlock {
    pub(crate) width: u16,
    pub(crate) rows: Vec<Line<'static>>,
    /// False when physical width prevented semantic presentation from being represented completely.
    pub(crate) physically_complete: bool,
}

/// INTERNAL PRESENTATION MECHANICS. The sole owner of semantic width and row
/// layout for the new presentation path.
#[derive(Debug, Default)]
pub(crate) struct ViewCompiler {
    theme: ThemeResolver,
}

/// A compiled physical text row preserving resolved styles, display width, fit status, and source metadata.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledTextRow {
    pub(crate) line: Line<'static>,
    pub(crate) source_end: Option<usize>,
    pub(crate) fits: bool,
    pub(crate) width: usize,
}

impl ViewCompiler {
    pub(crate) fn compile(&self, view: &View, max_width: u16) -> LayoutBlock {
        let surface = self.layout(view, max_width, ResolvedTextStyle::default());
        let physically_complete = surface.physically_complete;
        LayoutBlock {
            width: surface.width,
            rows: lower_surface(surface),
            physically_complete,
        }
    }

    /// Shared text compiler used by both ordinary [`View`] layout and streaming provenance compilation.
    pub(crate) fn compile_projected_text_with_metadata(
        &self,
        text: &ProjectedText,
        max_width: u16,
    ) -> (u16, Vec<CompiledTextRow>) {
        self.compile_projected_text_with_style(text, max_width, ResolvedTextStyle::default())
    }

    fn compile_projected_text_with_style(
        &self,
        text: &ProjectedText,
        max_width: u16,
        inherited: ResolvedTextStyle,
    ) -> (u16, Vec<CompiledTextRow>) {
        use crate::presentation::stream::ProjectedTextLayout;
        use crate::presentation::wrap::wrap_styled_lines;
        if let ProjectedTextLayout::Hanging {
            body_column,
            prefix,
            prefix_style,
            show_prefix,
            ..
        } = &text.layout
        {
            let body_start = text
                .runs
                .first()
                .map_or(text.content_range.end, |run| run.owned.start);
            let body = ProjectedText {
                content_range: StreamRange::new(body_start, text.content_range.end),
                terminator: text.terminator,
                width: WidthRule::Fill,
                wrap: text.wrap,
                align: text.align,
                layout: ProjectedTextLayout::Plain,
                runs: text.runs.clone(),
            };
            let body_width = max_width.saturating_sub(*body_column).max(1);
            let (_, body_rows) =
                self.compile_projected_text_with_style(&body, body_width, inherited);
            let prefix_width = UnicodeWidthStr::width(prefix.as_str());
            let mut rows = Vec::with_capacity(body_rows.len());
            for (index, mut row) in body_rows.into_iter().enumerate() {
                let indent = if index == 0 && *show_prefix {
                    prefix.clone()
                } else {
                    " ".repeat(usize::from(*body_column))
                };
                let prefix_style = if index == 0 && *show_prefix {
                    self.theme
                        .resolve_text_style(inherited, prefix_style)
                        .to_ratatui_style()
                } else {
                    inherited.to_ratatui_style()
                };
                let mut spans = vec![Span::styled(indent, prefix_style)];
                spans.extend(row.line.spans.drain(..));
                let mut merged: Vec<Span<'static>> = Vec::with_capacity(spans.len());
                for span in spans {
                    if let Some(last) = merged.last_mut()
                        && last.style == span.style
                    {
                        last.content.to_mut().push_str(span.content.as_ref());
                    } else {
                        merged.push(span);
                    }
                }
                row.line = Line::from(merged);
                row.width = row.width.saturating_add(if index == 0 && *show_prefix {
                    prefix_width
                } else {
                    usize::from(*body_column)
                });
                row.fits = row.width <= usize::from(max_width);
                row.source_end = row.source_end.map(|end| {
                    end + (body_start.as_u64() - text.content_range.start.as_u64()) as usize
                });
                rows.push(row);
            }
            return (max_width, rows);
        }

        let hard_lines = projected_hard_lines(&self.theme, text, inherited);

        let intrinsic_width = hard_lines
            .iter()
            .map(|line| line.iter().map(|atom| atom.width).sum::<usize>())
            .max()
            .unwrap_or(0);
        let width = match text.width {
            WidthRule::Fit => intrinsic_width.min(usize::from(max_width)) as u16,
            WidthRule::Fill => max_width,
        };
        let wrapped = wrap_styled_lines(&hard_lines, width, text.wrap);
        let rows = wrapped
            .into_iter()
            .map(|w_line| {
                let mut spans: Vec<Span<'static>> = Vec::new();
                for g in &w_line.graphemes {
                    if let Some(last) = spans.last_mut()
                        && last.style == g.style
                    {
                        last.content.to_mut().push_str(g.text.as_ref());
                    } else {
                        spans.push(Span::styled(g.text.to_string(), g.style));
                    }
                }
                CompiledTextRow {
                    line: Line::from(spans),
                    source_end: w_line
                        .graphemes
                        .last()
                        .and_then(|g| g.source.as_ref())
                        .map(|r| r.end),
                    fits: w_line.fits,
                    width: w_line.width,
                }
            })
            .collect();
        (width, rows)
    }

    fn compile_text_with_metadata(
        &self,
        text: &TextView,
        max_width: u16,
        width_rule: WidthRule,
        inherited: ResolvedTextStyle,
        track_source: bool,
    ) -> (u16, Vec<CompiledTextRow>) {
        let mut relative_source = 0usize;
        let spans = text.spans.iter().map(|span| {
            let base = if track_source {
                let current = relative_source;
                relative_source += span.text.len();
                Some(current)
            } else {
                None
            };
            (
                span.text.as_str(),
                self.theme
                    .resolve_text_style(inherited, &span.style)
                    .to_ratatui_style(),
                base,
            )
        });
        let hard_lines = styled_hard_lines(spans);
        let intrinsic_width = hard_lines
            .iter()
            .map(|line| line.iter().map(|grapheme| grapheme.width).sum::<usize>())
            .max()
            .unwrap_or(0);
        let width = match width_rule {
            WidthRule::Fit => intrinsic_width.min(usize::from(max_width)) as u16,
            WidthRule::Fill => max_width,
        };
        let wrapped = wrap_styled_lines(&hard_lines, width, text.wrap);
        let rows = wrapped
            .into_iter()
            .map(|w_line| {
                let mut spans: Vec<Span<'static>> = Vec::new();
                for g in &w_line.graphemes {
                    if let Some(last) = spans.last_mut()
                        && last.style == g.style
                    {
                        last.content.to_mut().push_str(g.text.as_ref());
                    } else {
                        spans.push(Span::styled(g.text.to_string(), g.style));
                    }
                }
                let source_end = w_line
                    .graphemes
                    .last()
                    .and_then(|g| g.source.as_ref())
                    .map(|r| r.end);
                CompiledTextRow {
                    line: Line::from(spans),
                    source_end,
                    fits: w_line.fits,
                    width: w_line.width,
                }
            })
            .collect();

        (width, rows)
    }

    fn layout(&self, view: &View, max_width: u16, inherited: ResolvedTextStyle) -> Surface {
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
        let requested_width = core.width.saturating_add(horizontal).min(u16::MAX);
        let width = match view.width {
            WidthRule::Fit => requested_width.min(max_width),
            WidthRule::Fill => max_width,
        };
        let height = core
            .height
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
        inherited: ResolvedTextStyle,
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
            // Spacer is vertical space and fills its allocated width. Its cells
            // remain transparent so a surrounding decorated node can paint through it.
            ViewKind::Spacer { rows } => Surface::new(max_width, *rows),
            ViewKind::ClampRows(clamp) => self.layout_clamp(clamp, max_width, inherited),
        }
    }

    fn layout_text(
        &self,
        width_rule: WidthRule,
        text: &TextView,
        max_width: u16,
        inherited: ResolvedTextStyle,
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
            let mut x = offset;
            for span in &row.line.spans {
                for g_text in span.content.graphemes(true) {
                    let g_width = UnicodeWidthStr::width(g_text);
                    if g_width == 0 {
                        continue;
                    }
                    if x >= usize::from(width) || x.saturating_add(g_width) > usize::from(width) {
                        break;
                    }
                    let cell = surface.get_mut(x as u16, y as u16);
                    cell.grapheme = Some(g_text.to_string());
                    cell.style = span.style;
                    cell.painted = true;
                    for continuation in 1..g_width {
                        let position = x + continuation;
                        if position >= usize::from(width) {
                            break;
                        }
                        let cell = surface.get_mut(position as u16, y as u16);
                        cell.grapheme = None;
                        cell.style = span.style;
                        cell.painted = true;
                        cell.continuation = true;
                    }
                    x += g_width;
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
        inherited: ResolvedTextStyle,
    ) -> Surface {
        let children = column
            .children
            .iter()
            .map(|child| self.layout(child, max_width, inherited))
            .collect::<Vec<_>>();
        let content_width = children.iter().map(|child| child.width).max().unwrap_or(0);
        let width = match width_rule {
            WidthRule::Fit => content_width.min(max_width),
            WidthRule::Fill => max_width,
        };
        let gap = usize::from(column.gap);
        let height = children
            .iter()
            .map(|child| usize::from(child.height))
            .sum::<usize>()
            .saturating_add(gap.saturating_mul(children.len().saturating_sub(1)));
        let mut output = Surface::new(width, height.min(usize::from(u16::MAX)) as u16);
        let mut y = 0u16;
        for child in children {
            output.composite(&child, 0, y);
            y = y.saturating_add(child.height).saturating_add(column.gap);
        }
        output
    }

    fn layout_row(
        &self,
        width_rule: WidthRule,
        row: &RowView,
        max_width: u16,
        inherited: ResolvedTextStyle,
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
            .map(|(_, child)| child.height)
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
                VerticalAlign::Center => content_height.saturating_sub(child.height) / 2,
                VerticalAlign::Bottom => content_height.saturating_sub(child.height),
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
        inherited: ResolvedTextStyle,
    ) -> Surface {
        let child = self.layout(&clamp.child, max_width, inherited);
        if child.height <= clamp.max_rows {
            return child;
        }
        if clamp.max_rows == 0 {
            return Surface::new(child.width, 0);
        }

        let mut output = Surface::new(child.width, clamp.max_rows);
        output.physically_complete = child.physically_complete;
        for y in 0..clamp.max_rows {
            for x in 0..child.width {
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
                self.layout(&indicator_view.into_view(), child.width, inherited);
            let row = clamp.max_rows - 1;
            for x in 0..child.width {
                *output.get_mut(x, row) = Cell::transparent();
                if x < indicator_surface.width {
                    *output.get_mut(x, row) = indicator_surface.get(x, 0).clone();
                }
            }
        }
        output
    }
}

#[derive(Debug, Clone)]
struct RowAllocation {
    tracks: Vec<u16>,
    gap: u16,
}

fn allocate_tracks(
    row: &RowView,
    width: u16,
    compiler: &ViewCompiler,
    inherited: ResolvedTextStyle,
) -> RowAllocation {
    let count = row.children.len();
    if count == 0 {
        return RowAllocation {
            tracks: Vec::new(),
            gap: 0,
        };
    }
    let gap_count = count.saturating_sub(1);
    let gap = if gap_count == 0 {
        0
    } else {
        row.gap.min(width / gap_count as u16)
    };
    let available = usize::from(width).saturating_sub(usize::from(gap) * gap_count);
    let mut tracks = vec![0u16; count];
    let mut used = 0usize;
    let mut flex = None;

    for (index, child) in row.children.iter().enumerate() {
        match child.track {
            TrackSize::Fixed(requested) => {
                let allocation = usize::from(requested).min(available.saturating_sub(used));
                tracks[index] = allocation as u16;
                used += allocation;
            }
            TrackSize::Content { max } => {
                let remaining = available.saturating_sub(used).min(usize::from(u16::MAX)) as u16;
                let preferred = compiler.layout(&child.view, remaining, inherited).width as usize;
                let allocation = preferred
                    .min(max.map_or(usize::MAX, usize::from))
                    .min(available.saturating_sub(used));
                tracks[index] = allocation as u16;
                used += allocation;
            }
            TrackSize::Flex { min } => {
                debug_assert!(flex.is_none(), "a row may contain at most one flex track");
                flex = Some((index, usize::from(min)));
            }
        }
    }

    if let Some((index, minimum)) = flex {
        let remaining = available.saturating_sub(used);
        tracks[index] = remaining.min(usize::from(u16::MAX)) as u16;
        let _minimum_is_satisfied = remaining >= minimum;
    }

    RowAllocation { tracks, gap }
}

use crate::presentation::wrap::{StyledGrapheme, styled_hard_lines, wrap_styled_lines};

fn paint_border(
    surface: &mut Surface,
    border: &BorderSpec,
    theme: &ThemeResolver,
    inherited: ResolvedTextStyle,
) {
    if surface.width == 0 || surface.height == 0 {
        return;
    }
    let style = border_style(border, theme, inherited);
    let (horizontal, vertical, corners) = match border.style {
        BorderStyle::Plain => ('─', '│', ('┌', '┐', '└', '┘')),
        BorderStyle::Rounded => ('─', '│', ('╭', '╮', '╰', '╯')),
        BorderStyle::Double => ('═', '║', ('╔', '╗', '╚', '╝')),
    };
    for x in 0..surface.width {
        set_cell(surface, x, 0, horizontal.to_string(), style, false);
        if surface.height > 1 {
            set_cell(
                surface,
                x,
                surface.height - 1,
                horizontal.to_string(),
                style,
                false,
            );
        }
    }
    for y in 0..surface.height {
        set_cell(surface, 0, y, vertical.to_string(), style, false);
        if surface.width > 1 {
            set_cell(
                surface,
                surface.width - 1,
                y,
                vertical.to_string(),
                style,
                false,
            );
        }
    }
    set_cell(surface, 0, 0, corners.0.to_string(), style, false);
    if surface.width > 1 {
        set_cell(
            surface,
            surface.width - 1,
            0,
            corners.1.to_string(),
            style,
            false,
        );
    }
    if surface.height > 1 {
        set_cell(
            surface,
            0,
            surface.height - 1,
            corners.2.to_string(),
            style,
            false,
        );
        if surface.width > 1 {
            set_cell(
                surface,
                surface.width - 1,
                surface.height - 1,
                corners.3.to_string(),
                style,
                false,
            );
        }
    }
}

fn border_style(border: &BorderSpec, theme: &ThemeResolver, inherited: ResolvedTextStyle) -> Style {
    let mut style = border
        .color
        .as_ref()
        .map(|color| Style::default().fg(theme.resolve_color(color)))
        .unwrap_or_else(|| inherited.to_ratatui_style());
    // Text backgrounds belong only to descendant text cells. Border cells
    // retain the backing surface background established before border paint.
    style.bg = None;
    style
}

fn set_cell(
    surface: &mut Surface,
    x: u16,
    y: u16,
    grapheme: String,
    mut style: Style,
    continuation: bool,
) {
    if x >= surface.width || y >= surface.height {
        return;
    }
    style.bg = surface.get(x, y).style.bg;
    let cell = surface.get_mut(x, y);
    cell.grapheme = Some(grapheme);
    cell.style = style;
    cell.painted = true;
    cell.continuation = continuation;
}

/// INTERNAL PRESENTATION MECHANICS. Width must always be supplied by the caller.
pub(crate) fn compile_view(view: &View, width: u16) -> LayoutBlock {
    ViewCompiler::default().compile(view, width)
}

pub(crate) fn view_height(view: &View, width: u16) -> u16 {
    compile_view(view, width)
        .rows
        .len()
        .min(usize::from(u16::MAX)) as u16
}

pub(crate) fn render_view(
    view: &View,
    buffer: &mut ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
) {
    let rows = compile_view(view, area.width).rows;
    for (index, row) in rows.iter().take(usize::from(area.height)).enumerate() {
        let y = area.y.saturating_add(index as u16);
        let row_area = ratatui::layout::Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };
        buffer.set_style(row_area, row.style);
        buffer.set_line(area.x, y, row, area.width);
    }
}

fn lower_surface(surface: Surface) -> Vec<Line<'static>> {
    let mut rows = Vec::with_capacity(usize::from(surface.height));
    for y in 0..surface.height {
        let last_painted = (0..surface.width)
            .rev()
            .find(|x| surface.get(*x, y).painted && !surface.get(*x, y).continuation);
        let Some(last_painted) = last_painted else {
            rows.push(Line::from(""));
            continue;
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        for x in 0..=last_painted {
            let cell = surface.get(x, y);
            if cell.continuation {
                continue;
            }
            let text = cell.grapheme.clone().unwrap_or_else(|| " ".to_string());
            if let Some(last) = spans.last_mut()
                && last.style == cell.style
            {
                last.content.to_mut().push_str(&text);
            } else {
                spans.push(Span::styled(text, cell.style));
            }
        }
        rows.push(Line::from(spans));
    }
    rows
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ResolvedTextStyle {
    foreground: Option<Color>,
    background: Option<Color>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    reversed: bool,
}

impl ResolvedTextStyle {
    fn to_ratatui_style(self) -> Style {
        let mut style = Style {
            fg: self.foreground,
            bg: self.background,
            ..Style::default()
        };
        let mut modifiers = Modifier::empty();
        if self.bold {
            modifiers |= Modifier::BOLD;
        }
        if self.dim {
            modifiers |= Modifier::DIM;
        }
        if self.italic {
            modifiers |= Modifier::ITALIC;
        }
        if self.underline {
            modifiers |= Modifier::UNDERLINED;
        }
        if self.reversed {
            modifiers |= Modifier::REVERSED;
        }
        style.add_modifier = modifiers;
        style
    }
}

#[derive(Debug, Default)]
struct ThemeResolver;

impl ThemeResolver {
    fn resolve_text_style(
        &self,
        inherited: ResolvedTextStyle,
        patch: &StyleSpec,
    ) -> ResolvedTextStyle {
        let mut resolved = inherited;
        if let Some(foreground) = &patch.foreground {
            resolved.foreground = Some(self.resolve_color(foreground));
        }
        if let Some(background) = &patch.background {
            resolved.background = Some(self.resolve_color(background));
        }
        if let Some(value) = patch.attributes.bold {
            resolved.bold = value;
        }
        if let Some(value) = patch.attributes.dim {
            resolved.dim = value;
        }
        if let Some(value) = patch.attributes.italic {
            resolved.italic = value;
        }
        if let Some(value) = patch.attributes.underline {
            resolved.underline = value;
        }
        if let Some(value) = patch.attributes.reversed {
            resolved.reversed = value;
        }
        resolved
    }

    fn resolve_color(&self, color: &ColorSpec) -> Color {
        match color {
            ColorSpec::Ansi(value) => Color::Indexed(*value),
            ColorSpec::Rgb { r, g, b } => Color::Rgb(*r, *g, *b),
            ColorSpec::Theme(key) => match key.0.as_str() {
                "surface.user" => theme::user_bubble_bg(),
                "text.muted" | "surface.default" => theme::muted().fg.unwrap_or(Color::Reset),
                "tool.running" => theme::tool_running().fg.unwrap_or(Color::Reset),
                "tool.finished" => theme::tool_finished().fg.unwrap_or(Color::Reset),
                "tool.error" | "text.error" => theme::tool_error().fg.unwrap_or(Color::Red),
                "text.warning" => Color::Yellow,
                "markdown.header" => theme::markdown_header().fg.unwrap_or(Color::Reset),
                "markdown.bold" => theme::markdown_bold().fg.unwrap_or(Color::Reset),
                "markdown.italic" => theme::markdown_italic().fg.unwrap_or(Color::Reset),
                "markdown.code" => theme::markdown_code().fg.unwrap_or(Color::Reset),
                "markdown.list" => theme::markdown_list().fg.unwrap_or(Color::Reset),
                _ => Color::Reset,
            },
        }
    }
}

#[derive(Debug, Clone)]
struct ProjectedExactFragment {
    display_start: usize,
    display_end: usize,
    style: Style,
    owned: StreamRange,
    visible: StreamRange,
}

/// Tokenizes adjacent exact projected runs as one visible string so an EGC
/// cannot be split merely because Markdown/style provenance changes at a run
/// boundary. Replacement runs remain explicit indivisible barriers.
fn projected_hard_lines(
    theme: &ThemeResolver,
    text: &ProjectedText,
    inherited: ResolvedTextStyle,
) -> Vec<Vec<StyledGrapheme<'static>>> {
    let mut hard_lines = vec![Vec::new()];
    let mut exact_display = String::new();
    let mut exact_fragments = Vec::new();

    for run in &text.runs {
        if run.display.is_empty() {
            continue;
        }
        let style = theme
            .resolve_text_style(inherited, &run.style)
            .to_ratatui_style();
        let Some(visible) = run.exact_visible else {
            append_projected_exact(
                &mut hard_lines,
                &exact_display,
                &exact_fragments,
                text.content_range.start,
            );
            exact_display.clear();
            exact_fragments.clear();
            hard_lines.last_mut().unwrap().push(StyledGrapheme {
                text: Cow::Owned(run.display.clone()),
                width: UnicodeWidthStr::width(run.display.as_str()),
                style,
                source: Some(
                    (run.owned.start.as_u64() - text.content_range.start.as_u64()) as usize
                        ..(run.owned.end.as_u64() - text.content_range.start.as_u64()) as usize,
                ),
            });
            continue;
        };

        let display_start = exact_display.len();
        exact_display.push_str(&run.display);
        exact_fragments.push(ProjectedExactFragment {
            display_start,
            display_end: exact_display.len(),
            style,
            owned: run.owned,
            visible,
        });
    }

    append_projected_exact(
        &mut hard_lines,
        &exact_display,
        &exact_fragments,
        text.content_range.start,
    );
    hard_lines
}

fn append_projected_exact(
    hard_lines: &mut Vec<Vec<StyledGrapheme<'static>>>,
    display: &str,
    fragments: &[ProjectedExactFragment],
    content_start: crate::presentation::StreamOffset,
) {
    for (relative, grapheme) in display.grapheme_indices(true) {
        let relative_end = relative + grapheme.len();
        let first = fragments
            .iter()
            .find(|fragment| {
                relative < fragment.display_end && relative_end > fragment.display_start
            })
            .expect("projected EGC must overlap an exact fragment");
        let last = fragments
            .iter()
            .rev()
            .find(|fragment| fragment.display_start < relative_end)
            .expect("projected EGC must overlap an exact fragment");

        let first_offset = relative - first.display_start;
        let source_start = if first_offset == 0 {
            first.owned.start
        } else {
            first.visible.start.saturating_add(first_offset as u64)
        };
        let last_offset_end = relative_end - last.display_start;
        let last_display_len = last.display_end - last.display_start;
        let source_end = if last_offset_end == last_display_len {
            last.owned.end
        } else {
            last.visible.start.saturating_add(last_offset_end as u64)
        };
        let mapped = StyledGrapheme {
            text: Cow::Owned(grapheme.to_string()),
            width: UnicodeWidthStr::width(grapheme),
            style: first.style,
            source: Some(
                (source_start.as_u64() - content_start.as_u64()) as usize
                    ..(source_end.as_u64() - content_start.as_u64()) as usize,
            ),
        };
        if grapheme == "\n" {
            hard_lines.push(Vec::new());
        } else {
            hard_lines.last_mut().unwrap().push(mapped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::api::style::{
        BorderSpec, BorderStyle, OverflowIndicator, TextAttribute,
    };
    use crate::presentation::stream::StreamRowCommit;
    use crate::presentation::{
        ColorSpec, Decoration, ExactTerminator, Insets, ProjectedTextLayout, ProjectedTextRun,
        RowChild, StreamNode, StreamOffset, StreamView, ThemeKey, WrapMode,
    };

    fn range(start: u64, end: u64) -> StreamRange {
        StreamRange::new(StreamOffset::new(start), StreamOffset::new(end))
    }

    fn text(row: &Line<'static>) -> String {
        row.spans.iter().map(|span| span.content.as_ref()).collect()
    }

    fn style(color: &str) -> StyleSpec {
        StyleSpec {
            foreground: Some(ColorSpec::Theme(ThemeKey::from(color))),
            ..StyleSpec::default()
        }
    }

    fn tool_view(body: &str) -> View {
        View::row(
            vec![
                RowChild::content(
                    View::text("●")
                        .no_wrap()
                        .style(style("tool.running"))
                        .into_view(),
                ),
                RowChild::flex(
                    View::text(body)
                        .style(style("text.default"))
                        .width(WidthRule::Fill)
                        .into_view(),
                ),
            ],
            1,
        )
    }

    #[test]
    fn row_uses_track_width_for_continuations() {
        let rows = compile_view(&tool_view("abcdefghijklmnop"), 10).rows;
        assert_eq!(text(&rows[0]), "● abcdefgh");
        assert_eq!(text(&rows[1]), "  ijklmnop");
    }

    #[test]
    fn narrow_rows_never_overflow_the_surface() {
        for width in 0..=4 {
            let view = View::row(
                vec![
                    RowChild::fixed(3, View::text("abc").into_view()),
                    RowChild::flex(View::text("body").width(WidthRule::Fill).into_view()),
                    RowChild::content(View::text("status").into_view()),
                ],
                2,
            );
            let block = compile_view(&view, width);
            assert!(block.width <= width);
            for row in block.rows {
                assert!(row.width() <= usize::from(width));
            }
        }
    }

    #[test]
    fn styled_spans_survive_wrapping_and_newlines() {
        let view = View::styled_text(vec![
            TextSpan::styled("abc", style("tool.running")),
            TextSpan::styled("def\ngh", style("tool.error")),
        ])
        .width(WidthRule::Fill)
        .into_view();
        let rows = compile_view(&view, 4).rows;
        assert_eq!(text(&rows[0]), "abcd");
        assert_eq!(text(&rows[1]), "ef");
        assert_eq!(text(&rows[2]), "gh");
        assert_eq!(rows[0].spans[0].style.fg, theme::tool_running().fg);
        assert_eq!(rows[0].spans[1].style.fg, theme::tool_error().fg);
        assert_eq!(rows[2].spans[0].style.fg, theme::tool_error().fg);
    }

    #[test]
    fn typed_text_style_cascades_to_physical_spans_without_rewriting_them() {
        let text = View::styled_text([
            TextSpan::plain("plain"),
            TextSpan::styled("bold", StyleSpec::new().bold()),
        ])
        .style(StyleSpec::new().foreground(ColorSpec::Ansi(1)))
        .into_view();
        let rows = compile_view(&text, 20).rows;

        assert_eq!(rows[0].spans[0].style.fg, Some(Color::Indexed(1)));
        assert!(!rows[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(rows[0].spans[1].style.fg, Some(Color::Indexed(1)));
        assert!(rows[0].spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn typed_text_wrap_and_no_wrap_preserve_existing_behavior() {
        let wrapped = View::text("abcd efgh")
            .wrap(WrapMode::WordThenGrapheme)
            .into_view();
        let grapheme = View::text("abcd efgh").wrap(WrapMode::Grapheme).into_view();
        let no_wrap = View::text("abcdef").no_wrap().into_view();

        let ViewKind::Text(wrapped_text) = wrapped.kind else {
            panic!("expected text view");
        };
        let ViewKind::Text(grapheme_text) = grapheme.kind else {
            panic!("expected text view");
        };
        assert_eq!(wrapped_text.wrap, WrapMode::WordThenGrapheme);
        assert_eq!(grapheme_text.wrap, WrapMode::Grapheme);
        assert!(
            !ViewCompiler::default()
                .compile(&no_wrap, 3)
                .physically_complete
        );
    }

    #[test]
    fn typed_text_alignment_uses_existing_text_layout() {
        for (align, expected) in [
            (HorizontalAlign::Start, "x"),
            (HorizontalAlign::Center, "  x"),
            (HorizontalAlign::End, "    x"),
        ] {
            let view = View::text("x")
                .width(WidthRule::Fill)
                .text_align(align)
                .into_view();
            let rows = compile_view(&view, 5).rows;
            assert_eq!(text(&rows[0]), expected);
        }
    }

    #[test]
    fn ancestor_and_child_text_styles_cascade_to_physical_text() {
        let mut child = View::text("x").into_view();
        child.decoration.text_style = StyleSpec::new().foreground(ColorSpec::Ansi(2));
        let mut view = View::box_(child, Decoration::default());
        view.decoration.text_style = StyleSpec::new().foreground(ColorSpec::Ansi(1));

        let surface = ViewCompiler::default().layout(&view, 1, ResolvedTextStyle::default());
        assert_eq!(surface.get(0, 0).style.fg, Some(Color::Indexed(2)));
    }

    #[test]
    fn span_style_overrides_node_and_explicit_false_cascades() {
        let mut child = View::styled_text(vec![
            TextSpan::plain("a"),
            TextSpan::styled("b", StyleSpec::new().bold()),
        ])
        .into_view();
        child.decoration.text_style = StyleSpec::new().attribute(TextAttribute::Bold, false);
        let mut view = View::box_(child, Decoration::default());
        view.decoration.text_style = StyleSpec::new().bold();

        let surface = ViewCompiler::default().layout(&view, 2, ResolvedTextStyle::default());
        assert!(
            !surface
                .get(0, 0)
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(
            surface
                .get(1, 0)
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn surface_background_paints_text_backing_and_transparent_tail() {
        let mut view = View::text("x").width(WidthRule::Fill).into_view();
        view.decoration.surface_background = Some(ColorSpec::Ansi(1));
        let surface = ViewCompiler::default().layout(&view, 4, ResolvedTextStyle::default());

        assert_eq!(surface.get(0, 0).style.bg, Some(Color::Indexed(1)));
        assert_eq!(surface.get(3, 0).style.bg, Some(Color::Indexed(1)));
        assert!(surface.get(3, 0).painted);
    }

    #[test]
    fn explicit_border_color_preserves_surface_background() {
        let mut decoration = Decoration::background(ColorSpec::Ansi(1));
        decoration.border = Some(BorderSpec {
            style: BorderStyle::Plain,
            color: Some(ColorSpec::Ansi(2)),
        });
        let view = View::box_(
            View::text("x").width(WidthRule::Fill).into_view(),
            decoration,
        )
        .width(WidthRule::Fill);
        let surface = ViewCompiler::default().layout(&view, 5, ResolvedTextStyle::default());

        let border = surface.get(0, 1).style;
        assert_eq!(border.fg, Some(Color::Indexed(2)));
        assert_eq!(border.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn implicit_border_color_preserves_surface_background_and_inherits_foreground() {
        let mut decoration = Decoration::background(ColorSpec::Ansi(1));
        decoration.text_style = StyleSpec::new().foreground(ColorSpec::Ansi(2));
        decoration.border = Some(BorderSpec {
            style: BorderStyle::Plain,
            color: None,
        });
        let view = View::box_(
            View::text("x").width(WidthRule::Fill).into_view(),
            decoration,
        )
        .width(WidthRule::Fill);
        let surface = ViewCompiler::default().layout(&view, 5, ResolvedTextStyle::default());

        let border = surface.get(0, 1).style;
        assert_eq!(border.fg, Some(Color::Indexed(2)));
        assert_eq!(border.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn text_background_does_not_leak_into_border() {
        let mut decoration = Decoration::default();
        decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
        decoration.border = Some(BorderSpec {
            style: BorderStyle::Plain,
            color: None,
        });
        let view = View::box_(
            View::text("x").width(WidthRule::Fill).into_view(),
            decoration,
        )
        .width(WidthRule::Fill);
        let surface = ViewCompiler::default().layout(&view, 5, ResolvedTextStyle::default());

        assert_eq!(surface.get(1, 1).style.bg, Some(Color::Indexed(2)));
        assert_eq!(surface.get(0, 1).style.bg, None);
    }

    #[test]
    fn surface_and_text_backgrounds_coexist_across_border_and_content() {
        let mut decoration = Decoration::background(ColorSpec::Ansi(1));
        decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
        decoration.border = Some(BorderSpec {
            style: BorderStyle::Plain,
            color: None,
        });
        let view = View::box_(
            View::text("x").width(WidthRule::Fill).into_view(),
            decoration,
        )
        .width(WidthRule::Fill);
        let surface = ViewCompiler::default().layout(&view, 5, ResolvedTextStyle::default());

        assert_eq!(surface.get(1, 1).style.bg, Some(Color::Indexed(2)));
        assert_eq!(surface.get(0, 1).style.bg, Some(Color::Indexed(1)));
        assert_eq!(surface.get(4, 1).style.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn border_painting_preserves_tiny_width_geometry() {
        let mut decoration = Decoration::background(ColorSpec::Ansi(1));
        decoration.border = Some(BorderSpec {
            style: BorderStyle::Plain,
            color: Some(ColorSpec::Ansi(2)),
        });
        let view = View::box_(View::text("x").into_view(), decoration).width(WidthRule::Fill);

        for width in [0, 1, 2, 3, 10] {
            let block = compile_view(&view, width);
            assert!(block.width <= width);
            assert!(
                block
                    .rows
                    .iter()
                    .all(|row| row.width() <= usize::from(width))
            );
        }
    }

    #[test]
    fn text_background_only_paints_text_cells() {
        let mut view = View::text("x").width(WidthRule::Fill).into_view();
        view.decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
        let surface = ViewCompiler::default().layout(&view, 4, ResolvedTextStyle::default());

        assert_eq!(surface.get(0, 0).style.bg, Some(Color::Indexed(2)));
        assert!(!surface.get(3, 0).painted);
    }

    #[test]
    fn explicit_text_background_wins_over_surface_background() {
        let mut view = View::text("x").width(WidthRule::Fill).into_view();
        view.decoration.surface_background = Some(ColorSpec::Ansi(1));
        view.decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
        let surface = ViewCompiler::default().layout(&view, 4, ResolvedTextStyle::default());

        assert_eq!(surface.get(0, 0).style.bg, Some(Color::Indexed(2)));
        assert_eq!(surface.get(3, 0).style.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn nested_surface_backgrounds_preserve_child_region() {
        let child = View::box_(
            View::text("x").into_view(),
            Decoration::background(ColorSpec::Ansi(2)),
        );
        let outer =
            View::box_(child, Decoration::background(ColorSpec::Ansi(1))).width(WidthRule::Fill);
        let surface = ViewCompiler::default().layout(&outer, 4, ResolvedTextStyle::default());

        assert_eq!(surface.get(0, 0).style.bg, Some(Color::Indexed(2)));
        assert_eq!(surface.get(3, 0).style.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn transparent_padding_shows_ancestor_surface_background() {
        let child = View::box_(
            View::text("x").into_view(),
            Decoration::default().padding(Insets::all(1)),
        );
        let outer =
            View::box_(child, Decoration::background(ColorSpec::Ansi(1))).width(WidthRule::Fill);
        let surface = ViewCompiler::default().layout(&outer, 5, ResolvedTextStyle::default());

        assert_eq!(surface.get(0, 0).style.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn surface_background_does_not_enter_text_style_cascade() {
        let mut view = View::text("x").into_view();
        view.decoration.surface_background = Some(ColorSpec::Ansi(1));
        let resolved = ViewCompiler::default()
            .theme
            .resolve_text_style(ResolvedTextStyle::default(), &view.decoration.text_style);
        assert_eq!(resolved.background, None);
    }

    #[test]
    fn projected_egc_spans_exact_run_boundaries_and_history_ownership() {
        let compiler = ViewCompiler::default();
        let projected = ProjectedText {
            content_range: range(0, 12),
            terminator: ExactTerminator::None,
            width: WidthRule::Fit,
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            layout: ProjectedTextLayout::Plain,
            runs: vec![
                ProjectedTextRun {
                    display: "a".to_string(),
                    style: style("markdown.bold"),
                    owned: range(0, 3),
                    exact_visible: Some(range(2, 3)),
                },
                ProjectedTextRun {
                    display: "\u{301} rest".to_string(),
                    style: style("text.default"),
                    owned: range(3, 12),
                    exact_visible: Some(range(5, 12)),
                },
            ],
        };
        let (_, rows) = compiler.compile_projected_text_with_metadata(&projected, 1);
        assert_eq!(text(&rows[0].line), "a\u{301}");
        assert_eq!(rows[0].source_end, Some(7));

        let view = StreamView::new(vec![StreamNode::projected_text(projected)]);
        let compiled = crate::presentation::stream::compile_stream(
            &view,
            1,
            crate::presentation::StreamOffset::new(12),
        );
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(7))
        );
    }

    #[test]
    fn projected_egc_spans_zwj_run_boundaries_without_splitting() {
        let first = "👩";
        let second = "\u{200d}💻";
        let split = first.len() as u64;
        let end = split + second.len() as u64;
        let projected = ProjectedText {
            content_range: range(0, end),
            terminator: ExactTerminator::None,
            width: WidthRule::Fit,
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            layout: ProjectedTextLayout::Plain,
            runs: vec![
                ProjectedTextRun {
                    display: first.to_string(),
                    style: style("markdown.bold"),
                    owned: range(0, split),
                    exact_visible: Some(range(0, split)),
                },
                ProjectedTextRun {
                    display: second.to_string(),
                    style: style("markdown.italic"),
                    owned: range(split, end),
                    exact_visible: Some(range(split, end)),
                },
            ],
        };
        let (_, rows) = ViewCompiler::default().compile_projected_text_with_metadata(&projected, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(text(&rows[0].line), format!("{first}{second}"));
    }

    #[test]
    fn projected_replacement_remains_one_indivisible_atom() {
        let projected = ProjectedText {
            content_range: range(0, 7),
            terminator: ExactTerminator::None,
            width: WidthRule::Fit,
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            layout: ProjectedTextLayout::Plain,
            runs: vec![
                ProjectedTextRun {
                    display: "foo".to_string(),
                    style: StyleSpec::default(),
                    owned: range(0, 3),
                    exact_visible: Some(range(0, 3)),
                },
                ProjectedTextRun {
                    display: "    ".to_string(),
                    style: StyleSpec::default(),
                    owned: range(3, 4),
                    exact_visible: None,
                },
                ProjectedTextRun {
                    display: "bar".to_string(),
                    style: StyleSpec::default(),
                    owned: range(4, 7),
                    exact_visible: Some(range(4, 7)),
                },
            ],
        };
        let (_, rows) = ViewCompiler::default().compile_projected_text_with_metadata(&projected, 1);
        assert!(rows.iter().any(|row| text(&row.line) == "    "));
    }

    #[test]
    fn projected_egc_boundaries_still_expose_independent_checkpoints() {
        let projected = ProjectedText {
            content_range: range(0, 2),
            terminator: ExactTerminator::None,
            width: WidthRule::Fit,
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            layout: ProjectedTextLayout::Plain,
            runs: vec![
                ProjectedTextRun {
                    display: "a".to_string(),
                    style: style("markdown.bold"),
                    owned: range(0, 1),
                    exact_visible: Some(range(0, 1)),
                },
                ProjectedTextRun {
                    display: "b".to_string(),
                    style: style("markdown.italic"),
                    owned: range(1, 2),
                    exact_visible: Some(range(1, 2)),
                },
            ],
        };
        let compiled = crate::presentation::stream::compile_stream(
            &StreamView::new(vec![StreamNode::projected_text(projected)]),
            1,
            crate::presentation::StreamOffset::new(2),
        );
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(1))
        );
        assert_eq!(
            compiled.commit[1],
            StreamRowCommit::Exact(StreamOffset::new(2))
        );
    }

    #[test]
    fn default_decoration_keeps_core_tails_transparent() {
        let compiler = ViewCompiler::default();
        let views = [
            View::text("a").width(WidthRule::Fill).into_view(),
            View::column(vec![View::text("a").width(WidthRule::Fill).into_view()], 0),
            View::row(vec![RowChild::content(View::text("a").into_view())], 0),
            View::spacer(1).width(WidthRule::Fill),
            View::clamp_rows(
                View::text("a").width(WidthRule::Fill).into_view(),
                1,
                OverflowIndicator::None,
            ),
        ];

        for (index, view) in views.into_iter().enumerate() {
            let surface = compiler.layout(&view, 4, ResolvedTextStyle::default());
            if index == 3 {
                assert!(surface.cells.iter().all(|cell| !cell.painted));
            } else {
                assert!(surface.cells.iter().any(|cell| cell.painted));
                assert!(!surface.get(3, 0).painted);
            }
        }
    }

    #[test]
    fn decorated_shell_paints_through_transparent_core() {
        let view = View::box_(
            View::spacer(1).width(WidthRule::Fill),
            Decoration::background(ColorSpec::Ansi(1)),
        );
        let surface = ViewCompiler::default().layout(&view, 3, ResolvedTextStyle::default());

        assert!(surface.get(0, 0).painted);
        assert_eq!(surface.get(0, 0).style.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn explicit_child_paint_wins_over_outer_background() {
        let child = View::styled_text(vec![TextSpan::styled(
            "x",
            StyleSpec {
                background: Some(ColorSpec::Ansi(2)),
                ..StyleSpec::default()
            },
        )])
        .width(WidthRule::Fill)
        .into_view();
        let view = View::box_(child, Decoration::background(ColorSpec::Ansi(1)));
        let surface = ViewCompiler::default().layout(&view, 3, ResolvedTextStyle::default());

        assert_eq!(surface.get(0, 0).style.bg, Some(Color::Indexed(2)));
        assert_eq!(surface.get(2, 0).style.bg, Some(Color::Indexed(1)));
    }

    #[test]
    fn decoration_preserves_physical_incompleteness() {
        let view = View::box_(
            View::text("漢").into_view(),
            Decoration::background(ColorSpec::Ansi(1)),
        );
        let compiler = ViewCompiler::default();

        assert!(!compiler.compile(&view, 1).physically_complete);
        assert!(compiler.compile(&view, 2).physically_complete);
    }

    #[test]
    fn box_background_covers_padding_and_row_gap() {
        let view = View::box_(
            tool_view("body"),
            Decoration::background(ColorSpec::Theme(ThemeKey::from("surface.user")))
                .padding(Insets::all(1)),
        );
        let rows = compile_view(&view, 12).rows;
        assert!(
            rows.iter()
                .all(|row| row.spans.iter().any(|span| span.style.bg.is_some()))
        );
    }

    #[test]
    fn ordinary_view_does_not_partially_paint_wide_grapheme() {
        let compiler = ViewCompiler::default();
        let view = View::text("漢").into_view();

        let block = compiler.compile(&view, 1);

        // Whatever the established clipped representation is,
        // it must not contain a half-painted wide grapheme.
        for row in &block.rows {
            for span in &row.spans {
                assert!(!span.content.contains('漢'));
            }
        }
    }

    #[test]
    fn clamp_emits_indicator() {
        let view = View::clamp_rows(
            View::text("one two three four").into_view(),
            2,
            crate::presentation::api::style::OverflowIndicator::Ellipsis {
                style: StyleSpec::default(),
            },
        );
        let rows = compile_view(&view, 4).rows;
        assert_eq!(rows.len(), 2);
        assert!(text(&rows[1]).contains('…'));
    }
}
