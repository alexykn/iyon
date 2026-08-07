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
        BorderStyle, BoxView, ColorSpec, ColumnView, Decoration, HorizontalAlign, MarkdownView,
        RowView, StyleSpec, TextAttributes, TextSpan, TextView, TrackSize, VerticalAlign, View,
        ViewKind, WidthRule, WrapMode,
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
}

/// INTERNAL PRESENTATION MECHANICS. The sole owner of semantic width and row
/// layout for the new presentation path.
#[derive(Debug, Default)]
pub(crate) struct ViewCompiler {
    theme: ThemeResolver,
}

impl ViewCompiler {
    pub(crate) fn compile(&self, view: &View, max_width: u16) -> LayoutBlock {
        let surface = self.layout(view, max_width, Style::default());
        LayoutBlock {
            width: surface.width,
            rows: lower_surface(surface),
        }
    }

    fn layout(&self, view: &View, max_width: u16, inherited: Style) -> Surface {
        match &view.kind {
            ViewKind::Text(text) => self.layout_text(view.width, text, max_width, inherited),
            ViewKind::Markdown(markdown) => {
                self.layout_markdown(view.width, markdown, max_width, inherited)
            }
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
        let spans = text
            .spans
            .iter()
            .map(|span| StyledSpan {
                text: &span.text,
                style: self.theme.resolve(&span.style, inherited),
            })
            .collect::<Vec<_>>();
        let hard_lines = styled_hard_lines(&spans);
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
        let mut surface = Surface::new(width, wrapped.len().max(1) as u16);
        for (y, line) in wrapped.iter().enumerate() {
            let line_width = line.iter().map(|grapheme| grapheme.width).sum::<usize>();
            let offset = match text.align {
                HorizontalAlign::Start => 0,
                HorizontalAlign::Center => usize::from(width).saturating_sub(line_width) / 2,
                HorizontalAlign::End => usize::from(width).saturating_sub(line_width),
            };
            let mut x = offset;
            for grapheme in line {
                if grapheme.width == 0 {
                    continue;
                }
                if x >= usize::from(width) || x.saturating_add(grapheme.width) > usize::from(width)
                {
                    break;
                }
                let cell = surface.get_mut(x as u16, y as u16);
                cell.grapheme = Some(grapheme.text.to_string());
                cell.style = grapheme.style;
                cell.painted = true;
                for continuation in 1..grapheme.width {
                    let position = x + continuation;
                    if position >= usize::from(width) {
                        break;
                    }
                    let cell = surface.get_mut(position as u16, y as u16);
                    cell.grapheme = None;
                    cell.style = grapheme.style;
                    cell.painted = true;
                    cell.continuation = true;
                }
                x += grapheme.width;
            }
        }
        surface
    }

    fn layout_markdown(
        &self,
        width_rule: WidthRule,
        markdown: &MarkdownView,
        max_width: u16,
        inherited: Style,
    ) -> Surface {
        // INCOMPLETE COMPATIBILITY PATH.
        //
        // The assistant's existing Markdown renderer remains authoritative because
        // it carries source/freeze metadata. No migrated feature should use this
        // generic Markdown adapter until that metadata-preserving adapter exists.
        let text = TextView {
            spans: vec![TextSpan {
                text: markdown.source.clone(),
                style: markdown.style.clone(),
            }],
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
        };
        self.layout_text(width_rule, &text, max_width, inherited)
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
        let width = match width_rule {
            WidthRule::Fit => allocation
                .tracks
                .iter()
                .map(|track| usize::from(*track))
                .sum::<usize>()
                .saturating_add(
                    usize::from(allocation.gap)
                        .saturating_mul(row.children.len().saturating_sub(1)),
                )
                .min(usize::from(u16::MAX)) as u16,
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
        let horizontal = decoration
            .padding
            .left
            .saturating_add(decoration.padding.right)
            .saturating_add(border.saturating_mul(2));
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
        let child_x = border.saturating_add(decoration.padding.left);
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
        // `min` is a preference when the terminal cannot satisfy it. Never
        // allocate beyond the remaining physical cells.
        tracks[index] = remaining.min(usize::from(u16::MAX)) as u16;
        let _minimum_is_satisfied = remaining >= minimum;
    }

    RowAllocation { tracks, gap }
}

#[derive(Clone, Copy)]
struct StyledGrapheme<'a> {
    text: &'a str,
    style: Style,
    width: usize,
}

struct StyledSpan<'a> {
    text: &'a str,
    style: Style,
}

fn styled_hard_lines<'a>(spans: &'a [StyledSpan<'a>]) -> Vec<Vec<StyledGrapheme<'a>>> {
    let mut lines = vec![Vec::new()];
    for span in spans {
        for grapheme in span.text.graphemes(true) {
            if grapheme == "\n" {
                lines.push(Vec::new());
                continue;
            }
            lines.last_mut().unwrap().push(StyledGrapheme {
                text: grapheme,
                style: span.style,
                width: grapheme.width(),
            });
        }
    }
    lines
}

fn wrap_styled_lines<'a>(
    lines: &[Vec<StyledGrapheme<'a>>],
    width: u16,
    mode: WrapMode,
) -> Vec<Vec<StyledGrapheme<'a>>> {
    let width = usize::from(width);
    let mut output = Vec::new();
    for line in lines {
        if mode == WrapMode::NoWrap || width == 0 {
            output.push(line.clone());
            continue;
        }
        if line.is_empty() {
            output.push(Vec::new());
            continue;
        }
        if mode == WrapMode::Grapheme {
            output.extend(split_graphemes(line, width));
            continue;
        }

        // WordThenGrapheme keeps a complete word when it fits. A word longer
        // than the track fills the remaining cells before continuing on the
        // next physical row; it must not discard usable cells after a prefix.
        let mut current = Vec::new();
        let mut current_width = 0usize;
        let mut token_start = 0usize;
        while token_start < line.len() {
            let mut token_end = token_start;
            while token_end < line.len() && !line[token_end].text.chars().all(char::is_whitespace) {
                token_end += 1;
            }
            while token_end < line.len() && line[token_end].text.chars().all(char::is_whitespace) {
                token_end += 1;
            }
            let token = &line[token_start..token_end];
            let token_width = token.iter().map(|grapheme| grapheme.width).sum::<usize>();
            if current_width > 0 && token_width <= width.saturating_sub(current_width) {
                current.extend_from_slice(token);
                current_width += token_width;
            } else if token_width <= width {
                if !current.is_empty() {
                    output.push(std::mem::take(&mut current));
                    current_width = 0;
                }
                current.extend_from_slice(token);
                current_width = token_width;
            } else {
                let mut rest = token;
                while !rest.is_empty() {
                    let available = width.saturating_sub(current_width);
                    let take = take_graphemes(rest, available.max(1));
                    current.extend_from_slice(&rest[..take]);
                    current_width += rest[..take]
                        .iter()
                        .map(|grapheme| grapheme.width)
                        .sum::<usize>();
                    rest = &rest[take..];
                    if current_width >= width {
                        output.push(std::mem::take(&mut current));
                        current_width = 0;
                    }
                }
            }
            token_start = token_end;
        }
        if !current.is_empty() {
            output.push(current);
        }
    }
    output
}

fn split_graphemes<'a>(line: &[StyledGrapheme<'a>], width: usize) -> Vec<Vec<StyledGrapheme<'a>>> {
    let mut output = Vec::new();
    let mut current = Vec::new();
    let mut used = 0usize;
    for grapheme in line {
        if used > 0 && used.saturating_add(grapheme.width) > width {
            output.push(std::mem::take(&mut current));
            used = 0;
        }
        current.push(*grapheme);
        used = used.saturating_add(grapheme.width);
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}

fn take_graphemes(line: &[StyledGrapheme<'_>], width: usize) -> usize {
    let mut used = 0usize;
    let mut count = 0usize;
    for grapheme in line {
        if count > 0 && used.saturating_add(grapheme.width) > width {
            break;
        }
        used = used.saturating_add(grapheme.width);
        count += 1;
        if used >= width {
            break;
        }
    }
    count.max(1).min(line.len())
}

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
        let rows = compile_view(&tool_view("$ abcdefghijklmnop"), 10).rows;
        assert_eq!(text(&rows[0]), "● $ abcdef");
        assert_eq!(text(&rows[1]), "  ghijklmn");
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
    fn clamp_emits_indicator() {
        let view = View::clamp_rows(
            View::text("one two three four"),
            2,
            crate::presentation::OverflowIndicator::Ellipsis {
                style: StyleSpec::default(),
            },
        );
        let rows = compile_view(&view, 4).rows;
        assert_eq!(rows.len(), 2);
        assert!(text(&rows[1]).contains('…'));
    }
}
