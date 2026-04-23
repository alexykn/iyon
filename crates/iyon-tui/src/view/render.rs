use std::ops::Range;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    input::{cursor_xy, wrapped_line_index_by_start},
    runtime::active::{ActivePaneKind, ActivePaneState, ActiveStreamState},
    view::RunningView,
};

const MAX_INPUT_HEIGHT: u16 = 13;

pub(crate) trait View {}

#[derive(Debug, Clone)]
pub(crate) struct SpacerView;
impl View for SpacerView {}

#[derive(Debug, Clone)]
pub(crate) struct InputView<'a> {
    pub(crate) text: &'a str,
    pub(crate) cursor_bytes: usize,
    pub(crate) scroll_rows: u16,
    pub(crate) wrapped_ranges: &'a [Range<usize>],
}
impl View for InputView<'_> {}

#[derive(Debug, Clone)]
pub(crate) struct ChatView<'a> {
    pub(crate) lines: &'a [Line<'static>],
}
impl View for ChatView<'_> {}

#[derive(Debug, Clone)]
pub(crate) struct ActiveView<'a> {
    pub(crate) active: Option<&'a ActivePaneState>,
}
impl View for ActiveView<'_> {}

#[derive(Debug, Clone)]
pub(crate) struct InfoView<'a> {
    pub(crate) status: &'a str,
}
impl View for InfoView<'_> {}

pub(crate) trait Renderable<V: View> {
    fn render(&self, view: &V, frame: &mut Frame, area: Rect);

    fn desired_height(&self, _: &V, area: Rect) -> u16 {
        let lines = Paragraph::new(Text::from(""))
            .wrap(Wrap { trim: false })
            .line_count(area.width.saturating_sub(2));
        u16::try_from(lines).unwrap_or(u16::MAX) + 2
    }
}

#[derive(Debug, Default)]
pub(crate) struct Renderer;

impl Renderer {
    pub(crate) fn draw_running(&self, frame: &mut Frame, view: &RunningView<'_>) {
        let spacer_view = SpacerView;
        self.render(&spacer_view, frame, view.layout.spacer_area);
        self.render(&view.chat, frame, view.layout.chat_area);
        self.render(&view.active, frame, view.layout.active_area);
        self.render(&view.input, frame, view.layout.input_area);
        self.render(&view.info, frame, view.layout.info_area);
    }

    pub(crate) fn next_input_scroll(
        &self,
        view: &InputView,
        area: Rect,
        current_scroll: u16,
    ) -> u16 {
        let block = Block::new().borders(Borders::TOP | Borders::BOTTOM);
        let inner = block.inner(area);
        let lines = view.wrapped_ranges;

        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }

        let visible_rows = usize::from(inner.height.max(1));
        let max_scroll = lines.len().saturating_sub(visible_rows);
        let mut next_scroll = usize::from(current_scroll).min(max_scroll);

        let cursor_line = wrapped_line_index_by_start(lines, cursor).unwrap_or(0);
        if cursor_line < next_scroll {
            next_scroll = cursor_line;
        }
        if cursor_line >= next_scroll + visible_rows {
            next_scroll = cursor_line + 1 - visible_rows;
        }

        u16::try_from(next_scroll).unwrap_or(u16::MAX)
    }
}

impl Renderable<SpacerView> for Renderer {
    fn render(&self, _view: &SpacerView, frame: &mut Frame, area: Rect) {
        frame.render_widget(ratatui::widgets::Paragraph::default(), area);
    }
}

impl Renderable<ActiveView<'_>> for Renderer {
    fn render(&self, view: &ActiveView, frame: &mut Frame, area: Rect) {
        let Some(active) = view.active else {
            frame.render_widget(Paragraph::default(), area);
            return;
        };

        let lines = match active.kind() {
            ActivePaneKind::WorkingSpinner => vec![
                Line::from(""),
                Line::from(format!("{} Working", active.spinner_frame())),
                Line::from(""),
            ],
            ActivePaneKind::AssistantStreaming => vec![Line::from(
                active
                    .stream()
                    .map(ActiveStreamState::active_tail)
                    .unwrap_or_default(),
            )],
            ActivePaneKind::SlashMenu => vec![Line::from("")],
            ActivePaneKind::FilePicker => vec![Line::from("")],
        };
        frame.render_widget(Paragraph::new(Text::from(lines)), area);
    }
}

impl Renderable<InputView<'_>> for Renderer {
    fn render(&self, view: &InputView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let mut block = Block::new()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(173, 216, 230)));

        let inner = block.inner(area);
        let content_width = inner.width.max(1);
        let lines = view.wrapped_ranges;

        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }

        let visible_rows = usize::from(inner.height.max(1));
        let scroll = usize::from(view.scroll_rows).min(lines.len().saturating_sub(visible_rows));
        if scroll > 0 {
            block = block
                .title(Line::from("────").left_aligned())
                .title(Line::from(format!(" ↑ {scroll} more ")).left_aligned());
        }
        let visible_end = (scroll + visible_rows).min(lines.len());

        let wrapped_text = lines[scroll..visible_end]
            .iter()
            .map(|range| Line::from(&view.text[range.clone()]))
            .collect::<Vec<_>>();

        let widget = Paragraph::new(Text::from(wrapped_text)).block(block);
        frame.render_widget(widget, area);

        if area.width == 0 || area.height == 0 {
            return;
        }

        let (x, y) = cursor_xy(view.text, cursor, lines, content_width);
        let y = y.saturating_sub(u16::try_from(scroll).unwrap_or(u16::MAX)) + inner.y;
        let x = x + inner.x;

        let max_x = area.x.saturating_add(area.width.saturating_sub(1));
        let max_y = area.y.saturating_add(area.height.saturating_sub(1));
        frame.set_cursor_position((x.min(max_x), y.min(max_y)));
    }

    fn desired_height(&self, view: &InputView<'_>, area: Rect) -> u16 {
        let content_width = area.width.max(1);
        let lines = view.wrapped_ranges;
        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let (_, cursor_row) = cursor_xy(view.text, cursor, lines, content_width);
        let visual_rows = (lines.len() as u16).max(cursor_row.saturating_add(1));
        visual_rows.saturating_add(2).min(MAX_INPUT_HEIGHT)
    }
}

impl Renderable<ChatView<'_>> for Renderer {
    fn render(&self, view: &ChatView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let buffer = frame.buffer_mut();
        let visible_rows = area.height as usize;
        let start = view.lines.len().saturating_sub(visible_rows);
        let visible_lines = &view.lines[start..];
        let top_padding = visible_rows.saturating_sub(visible_lines.len());

        for (row, line) in visible_lines.iter().enumerate() {
            let y = area.y + u16::try_from(top_padding + row).unwrap_or(u16::MAX);
            let row_area = Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            };

            buffer.set_style(row_area, line.style);
            buffer.set_line(area.x, y, line, area.width);
        }
    }

    fn desired_height(&self, view: &ChatView, _area: Rect) -> u16 {
        u16::try_from(view.lines.len()).unwrap_or(u16::MAX).max(1)
    }
}

impl Renderable<InfoView<'_>> for Renderer {
    fn render(&self, view: &InfoView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let widget = Paragraph::new(view.status).wrap(Wrap { trim: false });
        frame.render_widget(widget, area);
    }
}
