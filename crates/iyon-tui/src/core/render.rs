use std::{cell::RefCell, ops::Range};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    core::layout::ComputedLayout,
    util::wrapping::{compute_wrapped_ranges, cursor_xy, wrapped_line_index_by_start},
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
}
impl View for InputView<'_> {}

#[derive(Debug, Clone)]
pub(crate) struct ChatView<'a> {
    pub(crate) lines: &'a [Line<'static>],
}
impl View for ChatView<'_> {}

#[derive(Debug, Clone)]
pub(crate) struct InfoView<'a> {
    pub(crate) lines: &'a [Line<'static>],
}
impl View for InfoView<'_> {}

pub(crate) trait Renderable<V: View> {
    fn render(&self, view: &V, frame: &mut Frame, area: Rect);

    fn desired_height(&self, _: &V, area: Rect, input: &str) -> u16 {
        let lines = Paragraph::new(Text::from(input))
            .wrap(Wrap { trim: false })
            .line_count(area.width.saturating_sub(2));
        u16::try_from(lines).unwrap_or(u16::MAX) + 2
    }
}

#[derive(Debug, Default)]
pub(crate) struct Renderer {
    input_wrap_cache: RefCell<Option<WrapCache>>,
}

impl Renderer {
    pub(crate) fn draw_running(
        &self,
        frame: &mut Frame,
        rects: ComputedLayout,
        chat_view: &ChatView<'_>,
        input_view: &InputView<'_>,
        info_view: &InfoView<'_>,
    ) {
        let spacer_view = SpacerView;
        self.render(&spacer_view, frame, rects.spacer_area);
        self.render(chat_view, frame, rects.chat_area);
        self.render(&spacer_view, frame, rects.active_area);
        self.render(input_view, frame, rects.input_area);
        self.render(info_view, frame, rects.info_area);
    }

    pub(crate) fn draw_exit(
        &self,
        frame: &mut Frame,
        rects: ComputedLayout,
        chat_view: &ChatView<'_>,
        info_view: &InfoView<'_>,
        goodbye: &str,
    ) {
        let spacer_view = SpacerView;
        let input_view = InputView {
            text: goodbye,
            cursor_bytes: 0,
            scroll_rows: 0,
        };

        self.render(&spacer_view, frame, rects.spacer_area);
        self.render(chat_view, frame, rects.chat_area);
        self.render(&spacer_view, frame, rects.active_area);
        self.render(&input_view, frame, rects.input_area);
        self.render(info_view, frame, rects.info_area);
    }

    fn wrapped_ranges(&self, text: &str, width: u16) -> Vec<Range<usize>> {
        let mut cache = self.input_wrap_cache.borrow_mut();
        let recompute = match cache.as_ref() {
            Some(cached) => cached.width != width || cached.text != text,
            None => true,
        };

        if recompute {
            *cache = Some(WrapCache {
                width,
                lines: compute_wrapped_ranges(text, width),
                text: text.to_string(),
            });
        }

        cache.as_ref().map_or_else(
            || std::iter::once(0..0).collect(),
            |cached| cached.lines.clone(),
        )
    }

    pub(crate) fn format_info_lines(&self, status: &str) -> Vec<Line<'static>> {
        vec![Line::from(status.to_string())]
    }

    pub(crate) fn next_input_scroll(
        &self,
        view: &InputView,
        area: Rect,
        current_scroll: u16,
    ) -> u16 {
        let block = Block::new().borders(Borders::TOP | Borders::BOTTOM);
        let inner = block.inner(area);
        let content_width = inner.width.max(1);
        let lines = self.wrapped_ranges(view.text, content_width);

        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }

        let visible_rows = usize::from(inner.height.max(1));
        let max_scroll = lines.len().saturating_sub(visible_rows);
        let mut next_scroll = usize::from(current_scroll).min(max_scroll);

        let cursor_line = wrapped_line_index_by_start(&lines, cursor).unwrap_or(0);
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

impl Renderable<InputView<'_>> for Renderer {
    fn render(&self, view: &InputView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let mut block = Block::new()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(173, 216, 230)));

        let inner = block.inner(area);
        let content_width = inner.width.max(1);
        let lines = self.wrapped_ranges(view.text, content_width);

        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }

        let visible_rows = usize::from(inner.height.max(1));
        let scroll = usize::from(view.scroll_rows).min(lines.len().saturating_sub(visible_rows));
        if scroll > 0 {
            block = block
                .title(Line::from("────").left_aligned())
                .title(Line::from(format!(" ↑ {} more ", scroll)).left_aligned());
        }
        let visible_end = (scroll + visible_rows).min(lines.len());

        let wrapped_text = lines[scroll..visible_end]
            .iter()
            .map(|range| Line::from(&view.text[range.clone()]))
            .collect::<Vec<_>>();

        let widget = Paragraph::new(Text::from(wrapped_text)).block(block);
        frame.render_widget(widget, area);

        let (x, y) = cursor_xy(view.text, cursor, &lines, content_width);
        let y = y.saturating_sub(u16::try_from(scroll).unwrap_or(u16::MAX)) + inner.y;
        let x = x + inner.x;
        frame.set_cursor_position((x, y));
    }

    fn desired_height(&self, view: &InputView<'_>, area: Rect, _input: &str) -> u16 {
        let content_width = area.width.max(1);
        let lines = self.wrapped_ranges(view.text, content_width);
        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let (_, cursor_row) = cursor_xy(view.text, cursor, &lines, content_width);
        let visual_rows = (lines.len() as u16).max(cursor_row.saturating_add(1));
        visual_rows.saturating_add(2).min(MAX_INPUT_HEIGHT)
    }
}

impl Renderable<ChatView<'_>> for Renderer {
    fn render(&self, view: &ChatView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let buffer = frame.buffer_mut();
        let visible_rows = area.height as usize;
        let start = view.lines.len().saturating_sub(visible_rows);

        for (row, line) in view.lines[start..].iter().enumerate() {
            let y = area.y + u16::try_from(row).unwrap_or(u16::MAX);
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

    fn desired_height(&self, view: &ChatView, _area: Rect, _input: &str) -> u16 {
        u16::try_from(view.lines.len()).unwrap_or(u16::MAX).max(1)
    }
}

impl Renderable<InfoView<'_>> for Renderer {
    fn render(&self, view: &InfoView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let widget = Paragraph::new(Text::from(view.lines.to_vec())).wrap(Wrap { trim: false });
        frame.render_widget(widget, area);
    }
}

#[derive(Debug, Clone)]
struct WrapCache {
    width: u16,
    lines: Vec<Range<usize>>,
    text: String,
}
