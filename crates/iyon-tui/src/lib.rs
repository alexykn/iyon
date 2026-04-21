use anyhow::Result;
use crossterm::event::{self, Event, KeyCode::*, KeyEvent, KeyEventKind::*, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::cell::RefCell;
use std::ops::Range;
use std::time::Duration;
use textwrap::Options;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    spacer_height: u16,
    chat_height: u16,
    input_height: u16,
    info_height: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            spacer_height: 0,
            chat_height: 1,
            input_height: 3,
            info_height: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComputedLayout {
    pub spacer_area: Rect,
    pub chat_area: Rect,
    pub input_area: Rect,
    pub info_area: Rect,
}

impl LayoutConfig {
    pub fn compute(&self, area: Rect, config: &LayoutConfig) -> ComputedLayout {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(config.spacer_height),
                Constraint::Length(config.chat_height),
                Constraint::Length(config.input_height),
                Constraint::Length(config.info_height),
            ])
            .split(area);

        ComputedLayout {
            spacer_area: vertical[0],
            chat_area: vertical[1],
            input_area: vertical[2],
            info_area: vertical[3],
        }
    }
}

#[derive(Debug, Default)]
pub struct App {
    input_handler: InputEventHandler,
    layout: LayoutConfig,
    renderer: Renderer,
}

#[derive(Debug, Default)]
pub struct AppState {
    input: InputBuffer,
    chat: ChatState,
    info: InfoState,
    input_content_width: u16,
    exit: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ChatState {
    pub lines: Vec<Line<'static>>,
}

#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub text: String,
    pub cursor_bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InfoState {
    pub status: String,
}

impl InfoState {
    pub fn set_status(&mut self, status: &str) {
        self.status = status.to_string()
    }

    pub fn as_view<'a>(&'a self, lines: &'a [Line<'static>]) -> InfoView<'a> {
        InfoView { lines }
    }
}

impl ChatState {
    pub fn push_message(&mut self, msg: &str) {
        self.lines.push(Line::from(msg.to_string()));
    }

    pub fn as_view(&self) -> ChatView<'_> {
        ChatView {
            lines: self.lines.as_slice(),
        }
    }
}

pub trait View {}

#[derive(Debug, Clone)]
pub struct SpacerView;
impl View for SpacerView {}

#[derive(Debug, Clone)]
pub struct InputView<'a> {
    pub text: &'a str,
    pub cursor_bytes: usize,
}
impl View for InputView<'_> {}

#[derive(Debug, Clone)]
pub struct ChatView<'a> {
    pub lines: &'a [Line<'static>],
}
impl View for ChatView<'_> {}

#[derive(Debug, Clone)]
pub struct InfoView<'a> {
    pub lines: &'a [Line<'static>],
}
impl View for InfoView<'_> {}

pub trait Renderable<V: View> {
    fn render(&self, view: &V, frame: &mut Frame, area: Rect);

    fn desired_height(&self, _: &V, area: Rect, input: &str) -> u16 {
        let lines = Paragraph::new(Text::from(input))
            .wrap(Wrap { trim: false })
            .line_count(area.width.saturating_sub(2));
        u16::try_from(lines).unwrap_or(u16::MAX) + 2
    }
}

#[derive(Debug, Default)]
pub struct Renderer {
    input_wrap_cache: RefCell<Option<WrapCache>>,
}

impl Renderer {
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

    fn format_info_lines(&self, status: &str) -> Vec<Line<'static>> {
        vec![Line::from(status.to_string())]
    }
}

impl Renderable<SpacerView> for Renderer {
    fn render(&self, _view: &SpacerView, frame: &mut Frame, area: Rect) {
        let widget = ratatui::widgets::Paragraph::default();
        frame.render_widget(widget, area);
    }
}

impl<'a> Renderable<InputView<'a>> for Renderer {
    fn render(&self, view: &InputView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let block = Block::new()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(173, 216, 230)));
        let inner = block.inner(area);
        let content_width = inner.width.max(1);
        let lines = self.wrapped_ranges(view.text, content_width);
        let wrapped_text = lines
            .iter()
            .map(|range| Line::from(view.text[range.clone()].to_string()))
            .collect::<Vec<_>>();
        let widget = Paragraph::new(Text::from(wrapped_text))
            .wrap(Wrap { trim: false })
            .block(block);
        frame.render_widget(widget, area);

        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }

        let (x, y) = cursor_xy(view.text, cursor, &lines, content_width);
        let x = x + inner.x;
        let y = y + inner.y;
        frame.set_cursor_position((x, y));
    }

    fn desired_height(&self, view: &InputView<'a>, area: Rect, _input: &str) -> u16 {
        let content_w = area.width.max(1);
        let lines = self.wrapped_ranges(view.text, content_w);
        let mut cursor = view.cursor_bytes.min(view.text.len());
        while cursor > 0 && !view.text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let (_, cursor_row) = cursor_xy(view.text, cursor, &lines, content_w);
        let visual_rows = (lines.len() as u16).max(cursor_row.saturating_add(1));
        visual_rows.saturating_add(2)
    }
}

impl<'a> Renderable<ChatView<'a>> for Renderer {
    fn render(&self, view: &ChatView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let widget = Paragraph::new(Text::from(view.lines.to_vec())).wrap(Wrap { trim: false });
        frame.render_widget(widget, area);
    }

    fn desired_height(&self, view: &ChatView, area: Rect, _str: &str) -> u16 {
        let lines = Paragraph::new(Text::from(view.lines.to_vec()))
            .wrap(Wrap { trim: false })
            .line_count(area.width.max(1));
        u16::try_from(lines).unwrap_or(u16::MAX).max(1)
    }
}

impl<'a> Renderable<InfoView<'a>> for Renderer {
    fn render(&self, view: &InfoView, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let widget = Paragraph::new(Text::from(view.lines.to_vec())).wrap(Wrap { trim: false });
        frame.render_widget(widget, area);
    }
}

impl App {
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut state = AppState::default();

        while !state.exit {
            terminal.draw(|f| self.draw(f, &mut state))?;
            self.input_handler.run(&mut state)?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame, state: &mut AppState) {
        let root = frame.area();

        let spacer_view = SpacerView;

        // Pass 1: provisional layout (just to get input width)
        let base_cfg = LayoutConfig::default();
        let pass1 = self.layout.compute(root, &base_cfg);
        state.input_content_width = pass1.input_area.width.max(1);
        let info_lines = self.renderer.format_info_lines(&state.info.status);

        let input_view = InputView {
            text: &state.input.text,
            cursor_bytes: state.input.cursor,
        };
        let chat_view = ChatView {
            lines: &state.chat.lines,
        };
        let info_view = InfoView { lines: &info_lines };

        // Compute desired input height for this width
        let inpu_h = self
            .renderer
            .desired_height(&input_view, pass1.input_area, "");
        let chat_h = self
            .renderer
            .desired_height(&chat_view, pass1.chat_area, "");

        // Pass 2: final layout with dynamic input height
        let final_cfg = LayoutConfig {
            chat_height: chat_h,
            input_height: inpu_h,
            ..base_cfg
        };
        let rects = self.layout.compute(root, &final_cfg);

        // Render with final rects
        self.renderer.render(&spacer_view, frame, rects.spacer_area);
        self.renderer.render(&chat_view, frame, rects.chat_area);
        self.renderer.render(&input_view, frame, rects.input_area);
        self.renderer.render(&info_view, frame, rects.info_area);
    }
}

pub fn run() -> Result<()> {
    ratatui::run(|terminal| App::default().run(terminal))?;
    Ok(())
}

#[derive(Debug)]
enum CustomKeyEvent {
    None,
    Send,
    CtrlC,
    InsertText { text: String },
    InsertChar { char: char },
    InsertNewline,
    DeleteChar,
    DeleteWord,
    DeleteLine,
    Restore,
    MoveRightOne,
    MoveRightWord,
    MoveLineEnd,
    MoveUpOne,
    MoveDownOne,
    MoveLeftOne,
    MoveLeftWord,
    MoveLineStart,
}

fn map_key_event(key_event: KeyEvent) -> CustomKeyEvent {
    let code = key_event.code;
    let mods = key_event.modifiers;
    match (code, mods) {
        // C0 fallbacks some terminals emit without CONTROL modifier.
        (Char('\u{0002}'), KeyModifiers::NONE) => CustomKeyEvent::MoveLeftOne, // ^B
        (Char('\u{0006}'), KeyModifiers::NONE) => CustomKeyEvent::MoveRightOne, // ^F
        (Char('\u{0010}'), KeyModifiers::NONE) => CustomKeyEvent::MoveUpOne,   // ^P
        (Char('\u{000e}'), KeyModifiers::NONE) => CustomKeyEvent::MoveDownOne, // ^N

        // Send Input
        (Enter, m) if !m.contains(KeyModifiers::SHIFT) => CustomKeyEvent::Send,

        // Newline insertion.
        (Enter, m) if m == KeyModifiers::SHIFT => CustomKeyEvent::InsertNewline,
        (Char('j' | 'm'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::InsertNewline,

        // Edit commands.
        (Backspace, KeyModifiers::NONE) => CustomKeyEvent::DeleteChar,
        (Backspace, m) if m == KeyModifiers::CONTROL || m == KeyModifiers::ALT => {
            CustomKeyEvent::DeleteWord
        }
        (Char('h'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::DeleteChar,
        (Char('u'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::DeleteLine,
        (Char('c'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::CtrlC,
        (Char('y'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::Restore,

        // Movement.
        (Right, KeyModifiers::NONE) => CustomKeyEvent::MoveRightOne,
        (Right, m) if m == KeyModifiers::CONTROL || m == KeyModifiers::ALT => {
            CustomKeyEvent::MoveRightWord
        }
        (Char('f'), m) if m == KeyModifiers::ALT => CustomKeyEvent::MoveRightWord,
        (End, KeyModifiers::NONE) => CustomKeyEvent::MoveLineEnd,
        (Char('e'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::MoveLineEnd,
        (Up, KeyModifiers::NONE) => CustomKeyEvent::MoveUpOne,
        (Char('p'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::MoveUpOne,
        (Down, KeyModifiers::NONE) => CustomKeyEvent::MoveDownOne,
        (Char('n'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::MoveDownOne,
        (Left, KeyModifiers::NONE) => CustomKeyEvent::MoveLeftOne,
        (Left, m) if m == KeyModifiers::CONTROL || m == KeyModifiers::ALT => {
            CustomKeyEvent::MoveLeftWord
        }
        (Char('b'), m) if m == KeyModifiers::ALT => CustomKeyEvent::MoveLeftWord,
        (Home, KeyModifiers::NONE) => CustomKeyEvent::MoveLineStart,
        (Char('a'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::MoveLineStart,

        // Text insertion fallback:
        // accept plain, shift-modified and alt-modified chars; reject CONTROL combos.
        (Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
            CustomKeyEvent::InsertChar { char: c }
        }
        _ => CustomKeyEvent::None,
    }
}

#[derive(Debug, Default)]
struct InputEventHandler;

impl InputEventHandler {
    fn run(&mut self, app_state: &mut AppState) -> Result<()> {
        self.handle_event(event::read()?, app_state);

        // Drain immediately queued input so pastes that arrive as key bursts are
        // applied in one frame instead of animating char-by-char.
        while event::poll(Duration::from_millis(0))? {
            self.handle_event(event::read()?, app_state);
        }
        Ok(())
    }

    fn handle_event(&mut self, event: Event, app_state: &mut AppState) {
        match event {
            Event::Key(key_event) => {
                if !matches!(key_event.kind, Press | Repeat) {
                    return;
                }
                let custom_key_event = map_key_event(key_event);
                self.handle_key_event(custom_key_event, app_state);
            }

            Event::Paste(text) => {
                self.handle_key_event(CustomKeyEvent::InsertText { text }, app_state);
            }
            _ => {}
        }
    }

    fn handle_key_event(&mut self, custom_key_event: CustomKeyEvent, app_state: &mut AppState) {
        match custom_key_event {
            CustomKeyEvent::Send => {
                if app_state.input.text.is_empty() {
                    return;
                }
                app_state.chat.push_message(app_state.input.text.as_str());
                app_state.input.clear();
            }
            CustomKeyEvent::CtrlC => {
                if !app_state.input.text.is_empty() {
                    app_state.input.clear();
                    return;
                }
                app_state.exit = true;
            }
            CustomKeyEvent::InsertChar { char } => {
                app_state.input.insert_char(char);
            }
            CustomKeyEvent::InsertText { text } => {
                app_state.input.insert_str(&text);
            }
            CustomKeyEvent::InsertNewline => {
                app_state.input.insert_char('\n');
            }
            CustomKeyEvent::DeleteChar => {
                app_state.input.delete_char();
            }
            CustomKeyEvent::DeleteWord => {
                app_state.input.delete_word();
            }
            CustomKeyEvent::DeleteLine => {
                app_state.input.delete_line();
            }
            CustomKeyEvent::Restore => {
                app_state.input.restore();
            }
            CustomKeyEvent::MoveRightOne => {
                app_state.input.move_right();
            }
            CustomKeyEvent::MoveRightWord => {
                app_state.input.move_right_word();
            }
            CustomKeyEvent::MoveLineEnd => {
                app_state.input.move_line_end();
            }
            CustomKeyEvent::MoveUpOne => {
                app_state
                    .input
                    .move_up_visual(app_state.input_content_width);
            }
            CustomKeyEvent::MoveDownOne => {
                app_state
                    .input
                    .move_down_visual(app_state.input_content_width);
            }
            CustomKeyEvent::MoveLeftOne => {
                app_state.input.move_left();
            }
            CustomKeyEvent::MoveLeftWord => {
                app_state.input.move_left_word();
            }
            CustomKeyEvent::MoveLineStart => {
                app_state.input.move_line_start();
            }
            _ => (),
        }
    }
}

#[derive(Debug, Default)]
struct InputBuffer {
    text: String,
    cursor: usize,
    preferred_col: Option<usize>,
    kill_buffer: String,
}

impl InputBuffer {
    fn clamp_pos_to_char_boundary(&self, pos: usize) -> usize {
        let mut clamped = pos.min(self.text.len());
        while clamped > 0 && !self.text.is_char_boundary(clamped) {
            clamped -= 1;
        }
        clamped
    }

    fn set_cursor(&mut self, pos: usize) {
        self.cursor = self.clamp_pos_to_char_boundary(pos);
    }

    fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.set_cursor(self.cursor + s.len());
        self.preferred_col = None;
    }

    fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.set_cursor(self.cursor + c.len_utf8());
        self.preferred_col = None;
    }

    fn delete_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_boundary(self.cursor);
        self.text.drain(prev..self.cursor);
        self.set_cursor(prev);
        self.preferred_col = None;
    }

    fn delete_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_word_start(self.cursor);
        self.text.drain(prev..self.cursor);
        self.set_cursor(prev);
        self.preferred_col = None;
    }

    fn delete_line(&mut self) {
        let start = self.line_start(self.cursor);
        if self.cursor == start {
            if self.cursor == 0 {
                return;
            }
            // Cursor at beginning of line, jump up one line
            let prev = self.prev_line_end(self.cursor);
            self.text.drain(prev..self.cursor);
            self.set_cursor(prev);
            self.preferred_col = None;
            return; // <- important: prevents drain(start..cursor)
        }
        // Cursor anywhere else, delete to line start
        self.kill_buffer = self.text[start..self.cursor].to_string();
        self.text.drain(start..self.cursor);
        self.set_cursor(start);
        self.preferred_col = None;
    }

    fn restore(&mut self) {
        if self.kill_buffer.is_empty() {
            return;
        }
        self.insert_str(&self.kill_buffer.to_string());
    }

    fn clear(&mut self) {
        self.text.clear();
        self.set_cursor(0);
        self.preferred_col = None;
    }

    fn move_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        self.set_cursor(self.next_boundary(self.cursor));
        self.preferred_col = None;
    }

    fn move_right_word(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        self.set_cursor(self.next_word_start(self.cursor));
        self.preferred_col = None;
    }

    fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.set_cursor(self.prev_boundary(self.cursor));
        self.preferred_col = None;
    }

    fn move_left_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.set_cursor(self.prev_word_start(self.cursor));
        self.preferred_col = None;
    }

    fn move_line_start(&mut self) {
        self.set_cursor(self.line_start(self.cursor));
        self.preferred_col = None;
    }

    fn move_line_end(&mut self) {
        self.set_cursor(self.line_end(self.cursor));
        self.preferred_col = None;
    }

    fn move_up_visual(&mut self, width: u16) {
        let content_width = width.max(1);
        let lines = compute_wrapped_ranges(&self.text, content_width);
        let Some(line_idx) = wrapped_line_index_by_start(&lines, self.cursor) else {
            return;
        };

        if line_idx == 0 {
            self.set_cursor(0);
            return;
        }

        let current = &lines[line_idx];
        let target_col = self
            .preferred_col
            .unwrap_or_else(|| self.text[current.start..self.cursor].width());
        self.preferred_col = Some(target_col);

        let prev = &lines[line_idx - 1];
        self.set_cursor(cursor_for_display_col(
            &self.text, prev.start, prev.end, target_col,
        ));
    }

    fn move_down_visual(&mut self, width: u16) {
        let content_width = width.max(1);
        let lines = compute_wrapped_ranges(&self.text, content_width);
        let Some(line_idx) = wrapped_line_index_by_start(&lines, self.cursor) else {
            return;
        };

        if line_idx + 1 >= lines.len() {
            self.set_cursor(self.text.len());
            return;
        }

        let current = &lines[line_idx];
        let target_col = self
            .preferred_col
            .unwrap_or_else(|| self.text[current.start..self.cursor].width());
        self.preferred_col = Some(target_col);

        let next = &lines[line_idx + 1];
        self.set_cursor(cursor_for_display_col(
            &self.text, next.start, next.end, target_col,
        ));
    }

    fn line_start(&self, pos: usize) -> usize {
        let pos = self.clamp_pos_to_char_boundary(pos);
        self.text[..pos].rfind("\n").map_or(0, |idx| idx + 1)
    }

    fn line_end(&self, pos: usize) -> usize {
        let pos = self.clamp_pos_to_char_boundary(pos);
        self.text[pos..]
            .find("\n")
            .map_or(self.text.len(), |idx| pos + idx)
    }

    fn prev_line_end(&self, pos: usize) -> usize {
        let pos = self.clamp_pos_to_char_boundary(pos);
        self.text[..pos].rfind('\n').unwrap_or(pos)
    }

    fn prev_word_start(&self, mut pos: usize) -> usize {
        while pos > 0 {
            let prev = self.prev_boundary(pos);
            let ch = self.text[prev..pos].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            pos = prev;
        }

        while pos > 0 {
            let prev = self.prev_boundary(pos);
            let ch = self.text[prev..pos].chars().next().unwrap();
            if ch.is_whitespace() {
                break;
            }
            pos = prev;
        }
        pos
    }

    fn next_word_start(&self, mut pos: usize) -> usize {
        while pos < self.text.len() {
            let next = self.next_boundary(pos);
            let ch = self.text[pos..next].chars().next().unwrap();
            if ch.is_whitespace() {
                break;
            }
            pos = next;
        }
        while pos < self.text.len() {
            let next = self.next_boundary(pos);
            let ch = self.text[pos..next].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            pos = next;
        }
        pos
    }

    fn prev_boundary(&self, pos: usize) -> usize {
        self.text[..pos]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(idx, _)| idx)
    }

    fn next_boundary(&self, pos: usize) -> usize {
        self.text[pos..]
            .grapheme_indices(true)
            .next()
            .map_or(pos, |(_, grapheme)| pos + grapheme.len())
    }
}

#[derive(Debug, Clone)]
struct WrapCache {
    width: u16,
    lines: Vec<Range<usize>>,
    text: String,
}

fn cursor_xy(text: &str, cursor: usize, lines: &[Range<usize>], max_cols: u16) -> (u16, u16) {
    let mut i = lines
        .partition_point(|r| r.start <= cursor)
        .saturating_sub(1);
    let ls = &lines[i];
    let mut col = text[ls.start..cursor].width() as u16;

    if col >= max_cols {
        i += 1;
        col = 0;
    }

    (col, i as u16)
}

fn wrapped_line_index_by_start(lines: &[Range<usize>], pos: usize) -> Option<usize> {
    let idx = lines.partition_point(|r| r.start <= pos);
    if idx == 0 { None } else { Some(idx - 1) }
}

fn cursor_for_display_col(
    text: &str,
    line_start: usize,
    line_end: usize,
    target_col: usize,
) -> usize {
    let mut width_so_far = 0usize;
    for (i, g) in text[line_start..line_end].grapheme_indices(true) {
        width_so_far += g.width();
        if width_so_far > target_col {
            return line_start + i;
        }
    }
    line_end
}

fn compute_wrapped_ranges(text: &str, width: u16) -> Vec<Range<usize>> {
    if text.is_empty() {
        return vec![0..0];
    }

    let wrap_width = usize::from(width.saturating_sub(1).max(1));
    let opts = Options::new(wrap_width).break_words(true);
    let mut visual = Vec::<Range<usize>>::new();

    for logical in logical_line_ranges(text) {
        let slice = &text[logical.clone()];

        if slice.is_empty() {
            visual.push(logical.start..logical.start);
            continue;
        }

        // Track where we are in the original string's absolute memory
        let mut consumed_abs = logical.start;

        for piece in textwrap::wrap(slice, &opts) {
            let range = match piece {
                std::borrow::Cow::Borrowed(p) => {
                    // O(1) Memory pointer math (Zero guessing, 100% accurate)
                    let abs_start = unsafe { p.as_ptr().offset_from(text.as_ptr()) as usize };
                    let abs_end = abs_start + p.len();
                    abs_start..abs_end
                }
                std::borrow::Cow::Owned(p) => {
                    // Map character-by-character to skip inserted hyphens
                    map_owned_piece_to_range(text, consumed_abs, logical.end, &p)
                }
            };

            // Fix for "Trailing Space EOL Cursor Jumping":
            // textwrap strips trailing spaces. We must manually count how many spaces
            // were left behind in the source text and append them to this visual line's range.
            let trailing_spaces_bytes: usize = text[range.end..logical.end]
                .chars()
                .take_while(|c| *c == ' ')
                .map(|c| c.len_utf8())
                .sum();

            let final_end = range.end + trailing_spaces_bytes;
            visual.push(range.start..final_end);

            // Advance our tracker for the next piece
            consumed_abs = final_end;
        }
    }

    if visual.is_empty() {
        visual.push(0..0);
    }
    visual
}

fn map_owned_piece_to_range(
    text: &str,
    mut start: usize,
    max_end: usize,
    wrapped: &str,
) -> Range<usize> {
    // 1. textwrap drops spaces at breakpoints.
    // Advance `start` over any spaces in the source text that aren't present in the wrapped line.
    while start < max_end && !wrapped.starts_with(' ') {
        let Some(ch) = text[start..].chars().next() else {
            break;
        };
        if ch == ' ' {
            start += ch.len_utf8();
        } else {
            break;
        }
    }

    let mut end = start;
    let mut chars = wrapped.chars().peekable();

    // 2. Walk character by character
    while let Some(ch) = chars.next() {
        if end < max_end {
            let src_ch = text[end..].chars().next().unwrap();

            if ch == src_ch {
                // Character matches the source text exactly. Advance the source pointer.
                end += src_ch.len_utf8();
                continue;
            }
        }

        // 3. MISMATCH!
        // In a Cow::Owned string, this usually means textwrap inserted a synthetic hyphen
        // to break a long word.
        if ch == '-' && chars.peek().is_none() {
            // Ignore the hyphen, DO NOT advance the `end` source pointer.
            continue;
        }

        // If it's some other unexpected synthetic character, just ignore it and keep
        // trying to resync on the next character. This prevents panics.
    }

    start..end
}

fn logical_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0usize;

    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            out.push(start..i);
            start = i + 1
        }
    }
    out.push(start..text.len());
    out
}
