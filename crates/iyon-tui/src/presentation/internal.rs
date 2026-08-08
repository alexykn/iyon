//! INTERNAL PRESENTATION MECHANICS.
//!
//! This module is the future library boundary's terminal adapter. It resolves
//! semantic views once the actual width is known, composes them on a transparent
//! cell surface, and only then lowers them to final physical `Line`s.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    presentation::api::{
        BorderStyle, BoxView, ColorSpec, ColumnView, Decoration, HorizontalAlign, RowView,
        StyleSpec, TextAttributes, TextSpan, TextView, TrackSize, VerticalAlign, View, ViewKind,
        WidthRule,
    },
    presentation::stream::{ProjectedText, StreamRange},
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

    fn paint_background(&mut self, style: Style) {
        for cell in &mut self.cells {
            if !cell.painted {
                cell.style = style;
                cell.grapheme = None;
                cell.continuation = false;
            }
            cell.painted = true;
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
        let surface = self.layout(view, max_width, Style::default());
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
        inherited: Style,
    ) -> (u16, Vec<CompiledTextRow>) {
        use crate::presentation::stream::ProjectedTextLayout;
        use crate::presentation::wrap::{StyledGrapheme, wrap_styled_lines};
        use std::borrow::Cow;

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
                self.compile_projected_text_with_metadata(&body, body_width, inherited);
            let prefix_width = UnicodeWidthStr::width(prefix.as_str());
            let mut rows = Vec::with_capacity(body_rows.len());
            for (index, mut row) in body_rows.into_iter().enumerate() {
                let indent = if index == 0 && *show_prefix {
                    prefix.clone()
                } else {
                    " ".repeat(usize::from(*body_column))
                };
                let prefix_style = if index == 0 && *show_prefix {
                    self.theme.resolve(prefix_style, inherited)
                } else {
                    inherited
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

        let mut hard_lines: Vec<Vec<StyledGrapheme<'static>>> = vec![Vec::new()];
        for run in &text.runs {
            if run.display.is_empty() {
                continue;
            }
            let style = self.theme.resolve(&run.style, inherited);
            let Some(visible) = run.exact_visible else {
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

            for (relative, grapheme) in run.display.grapheme_indices(true) {
                let relative_end = relative + grapheme.len();
                let mut source_start = visible.start.as_u64() + relative as u64;
                let mut source_end = visible.start.as_u64() + relative_end as u64;
                if relative == 0 {
                    source_start = run.owned.start.as_u64();
                }
                if relative_end == run.display.len() {
                    source_end = run.owned.end.as_u64();
                }
                let mapped = StyledGrapheme {
                    text: Cow::Owned(grapheme.to_string()),
                    width: UnicodeWidthStr::width(grapheme),
                    style,
                    source: Some(
                        (source_start - text.content_range.start.as_u64()) as usize
                            ..(source_end - text.content_range.start.as_u64()) as usize,
                    ),
                };
                if grapheme == "\n" {
                    hard_lines.push(Vec::new());
                } else {
                    hard_lines.last_mut().unwrap().push(mapped);
                }
            }
        }

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

    pub(crate) fn compile_text_with_metadata(
        &self,
        text: &TextView,
        max_width: u16,
        width_rule: WidthRule,
        inherited: Style,
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
                self.theme.resolve(&span.style, inherited),
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

    fn layout(&self, view: &View, max_width: u16, inherited: Style) -> Surface {
        match &view.kind {
            ViewKind::Text(text) => self.layout_text(view.width, text, max_width, inherited),
            ViewKind::Column(column) => {
                self.layout_column(view.width, column, max_width, inherited)
            }
            ViewKind::Row(row) => self.layout_row(view.width, row, max_width, inherited),
            ViewKind::Box(box_view) => self.layout_box(view.width, box_view, max_width, inherited),
            // Spacer is vertical space and fills its allocated width. Its cells
            // remain transparent so a surrounding Box can paint through it.
            ViewKind::Spacer { rows } => Surface::new(max_width, *rows),
            ViewKind::ClampRows(clamp) => self.layout_clamp(clamp, max_width, inherited),
        }
    }

    fn layout_text(
        &self,
        width_rule: WidthRule,
        text: &TextView,
        max_width: u16,
        inherited: Style,
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
        inherited: Style,
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
        inherited: Style,
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

    fn layout_box(
        &self,
        width_rule: WidthRule,
        box_view: &BoxView,
        max_width: u16,
        inherited: Style,
    ) -> Surface {
        let decoration = &box_view.decoration;
        let resolved = self.theme.resolve_decoration(decoration, inherited);
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
        let child = self.layout(&box_view.child, inner_width, resolved);
        let requested_width = child.width.saturating_add(horizontal).min(u16::MAX);
        let width = match width_rule {
            WidthRule::Fit => requested_width.min(max_width),
            WidthRule::Fill => max_width,
        };
        let height = child
            .height
            .saturating_add(decoration.padding.top)
            .saturating_add(decoration.padding.bottom)
            .saturating_add(border.saturating_mul(2));
        let mut output = Surface::new(width, height);
        output.paint_background(resolved);
        let child_x = border.saturating_add(left_pad);
        let child_y = border.saturating_add(decoration.padding.top);
        output.composite(&child, child_x, child_y);
        if let Some(border_spec) = &decoration.border {
            paint_border(&mut output, border_spec, &self.theme, resolved);
        }
        output
    }

    fn layout_clamp(
        &self,
        clamp: &crate::presentation::api::ClampRowsView,
        max_width: u16,
        inherited: Style,
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
            crate::presentation::api::OverflowIndicator::None => None,
            crate::presentation::api::OverflowIndicator::Ellipsis { style } => {
                Some(("…".to_string(), style.clone()))
            }
            crate::presentation::api::OverflowIndicator::Footer { prefix, style } => {
                Some((prefix.clone(), style.clone()))
            }
        };
        if let Some((text, style)) = indicator {
            let indicator_view = View::styled_text(vec![TextSpan::styled(text, style)])
                .width(WidthRule::Fill)
                .no_wrap();
            let indicator_surface = self.layout(&indicator_view, child.width, inherited);
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
    inherited: Style,
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

use crate::presentation::wrap::{styled_hard_lines, wrap_styled_lines};

fn paint_border(
    surface: &mut Surface,
    border: &crate::presentation::api::BorderSpec,
    theme: &ThemeResolver,
    inherited: Style,
) {
    if surface.width == 0 || surface.height == 0 {
        return;
    }
    let style = border
        .color
        .as_ref()
        .map(|color| Style::default().fg(theme.resolve_color(color)))
        .unwrap_or(inherited);
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

fn set_cell(
    surface: &mut Surface,
    x: u16,
    y: u16,
    grapheme: String,
    style: Style,
    continuation: bool,
) {
    if x >= surface.width || y >= surface.height {
        return;
    }
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

#[derive(Debug, Default)]
struct ThemeResolver;

impl ThemeResolver {
    fn resolve(&self, spec: &StyleSpec, inherited: Style) -> Style {
        let mut style = inherited;
        if let Some(foreground) = &spec.foreground {
            style.fg = Some(self.resolve_color(foreground));
        }
        if let Some(background) = &spec.background {
            style.bg = Some(self.resolve_color(background));
        }
        apply_attributes(&mut style, spec.attributes);
        style
    }

    fn resolve_decoration(&self, decoration: &Decoration, inherited: Style) -> Style {
        self.resolve(
            &StyleSpec {
                foreground: decoration.foreground.clone(),
                background: decoration.background.clone(),
                attributes: decoration.attributes,
            },
            inherited,
        )
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

fn apply_attributes(style: &mut Style, attributes: TextAttributes) {
    let mut modifier = Modifier::empty();
    if attributes.bold {
        modifier |= Modifier::BOLD;
    }
    if attributes.dim {
        modifier |= Modifier::DIM;
    }
    if attributes.italic {
        modifier |= Modifier::ITALIC;
    }
    if attributes.underline {
        modifier |= Modifier::UNDERLINED;
    }
    if attributes.reversed {
        modifier |= Modifier::REVERSED;
    }
    style.add_modifier |= modifier;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::{ColorSpec, Decoration, Insets, RowChild, ThemeKey};

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
                RowChild::content(View::text("●").no_wrap().style(style("tool.running"))),
                RowChild::flex(
                    View::text(body)
                        .style(style("text.default"))
                        .width(WidthRule::Fill),
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
                    RowChild::fixed(3, View::text("abc")),
                    RowChild::flex(View::text("body").width(WidthRule::Fill)),
                    RowChild::content(View::text("status")),
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
        .width(WidthRule::Fill);
        let rows = compile_view(&view, 4).rows;
        assert_eq!(text(&rows[0]), "abcd");
        assert_eq!(text(&rows[1]), "ef");
        assert_eq!(text(&rows[2]), "gh");
        assert_eq!(rows[0].spans[0].style.fg, theme::tool_running().fg);
        assert_eq!(rows[0].spans[1].style.fg, theme::tool_error().fg);
        assert_eq!(rows[2].spans[0].style.fg, theme::tool_error().fg);
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
        let view = View::text("漢");

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
            View::text("one two three four"),
            2,
            crate::presentation::api::OverflowIndicator::Ellipsis {
                style: StyleSpec::default(),
            },
        );
        let rows = compile_view(&view, 4).rows;
        assert_eq!(rows.len(), 2);
        assert!(text(&rows[1]).contains('…'));
    }
}
