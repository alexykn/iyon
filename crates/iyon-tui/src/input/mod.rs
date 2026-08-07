use std::{ops::Range, time::Duration};

use anyhow::Result;
use crossterm::event::{
    self, Event,
    KeyCode::{BackTab, Backspace, Char, Down, End, Enter, Esc, Home, Left, Right, Tab, Up},
    KeyEvent,
    KeyEventKind::{Press, Repeat},
    KeyModifiers,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) mod wrap;

pub(crate) use wrap::{WrapCache, cursor_xy, wrapped_line_index_by_start};

use crate::{
    input::wrap::cursor_for_display_col, presentation::UiKey, runtime::controller::FrontendAction,
    runtime::panel::BottomPanelBar,
};

const WORD_SEPARATORS: &str = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";
const LARGE_PASTE_CHAR_THRESHOLD: usize = 1000;
const LARGE_PASTE_LINE_THRESHOLD: usize = 10;

#[derive(Debug)]
enum CustomKeyEvent {
    None,
    Send,
    CtrlC,
    InsertPaste { text: String },
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
    CycleReasoningEffort,
    Interrupt,
}

fn map_approval_key_event(key_event: KeyEvent) -> Option<FrontendAction> {
    match (key_event.code, key_event.modifiers) {
        (Enter | Char('y'), KeyModifiers::NONE) => Some(FrontendAction::ApprovePendingTool),
        (Esc | Char('n'), KeyModifiers::NONE) => Some(FrontendAction::RejectPendingTool),
        _ => None,
    }
}

fn ui_key(key: KeyEvent) -> UiKey {
    match key.code {
        Char(c) => UiKey::Char(c),
        Enter => UiKey::Enter,
        Esc => UiKey::Escape,
        Backspace => UiKey::Backspace,
        Tab | BackTab => UiKey::Tab,
        Up => UiKey::Up,
        Down => UiKey::Down,
        Left => UiKey::Left,
        Right => UiKey::Right,
        Home => UiKey::Home,
        End => UiKey::End,
        _ => UiKey::Unknown,
    }
}

fn map_key_event(key_event: KeyEvent) -> CustomKeyEvent {
    let code = key_event.code;
    let mods = key_event.modifiers;
    match (code, mods) {
        (Char('\u{0002}'), KeyModifiers::NONE) => CustomKeyEvent::MoveLeftOne,
        (Char('\u{0006}'), KeyModifiers::NONE) => CustomKeyEvent::MoveRightOne,
        (Char('\u{0010}'), KeyModifiers::NONE) => CustomKeyEvent::MoveUpOne,
        (Char('\u{000e}'), KeyModifiers::NONE) => CustomKeyEvent::MoveDownOne,

        (Enter, m) if !m.contains(KeyModifiers::SHIFT) => CustomKeyEvent::Send,

        (Enter, m) if m == KeyModifiers::SHIFT => CustomKeyEvent::InsertNewline,
        (Char('j' | 'm'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::InsertNewline,

        (Backspace, KeyModifiers::NONE) => CustomKeyEvent::DeleteChar,
        (Backspace, m) if m == KeyModifiers::CONTROL || m == KeyModifiers::ALT => {
            CustomKeyEvent::DeleteWord
        }
        (Char('h'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::DeleteChar,
        (Char('u'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::DeleteLine,
        (Char('\u{0003}'), _) => CustomKeyEvent::CtrlC,
        (Char('c'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::CtrlC,
        (Char('y'), m) if m == KeyModifiers::CONTROL => CustomKeyEvent::Restore,

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

        (BackTab, _) => CustomKeyEvent::CycleReasoningEffort,
        (Tab, m) if m.contains(KeyModifiers::SHIFT) => CustomKeyEvent::CycleReasoningEffort,
        (Esc, _) => CustomKeyEvent::Interrupt,

        (Char(c), m) => {
            let is_altgr = m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::ALT);
            let is_pure_ctrl = m.contains(KeyModifiers::CONTROL) && !is_altgr;
            let is_super_or_meta =
                m.contains(KeyModifiers::SUPER) || m.contains(KeyModifiers::META);

            if is_pure_ctrl || is_super_or_meta {
                CustomKeyEvent::None
            } else {
                CustomKeyEvent::InsertChar { char: c }
            }
        }
        _ => CustomKeyEvent::None,
    }
}

pub(crate) struct InputEditor<'a> {
    buffer: &'a mut InputBuffer,
    content_width: u16,
    wrap_cache: &'a mut WrapCache,
}

impl<'a> InputEditor<'a> {
    pub(crate) fn new(
        buffer: &'a mut InputBuffer,
        content_width: u16,
        wrap_cache: &'a mut WrapCache,
    ) -> Self {
        Self {
            buffer,
            content_width,
            wrap_cache,
        }
    }

    fn text(&self) -> &str {
        self.buffer.text()
    }

    fn clear(&mut self) {
        self.buffer.clear();
    }

    fn insert_char(&mut self, char: char) {
        self.buffer.insert_char(char);
    }

    fn insert_paste(&mut self, text: &str) {
        self.buffer.insert_paste(text);
    }

    fn submission_text(&self) -> String {
        self.buffer.submission_text()
    }

    fn delete_char(&mut self) {
        self.buffer.delete_char();
    }

    fn delete_word(&mut self) {
        self.buffer.delete_word();
    }

    fn delete_line(&mut self) {
        self.buffer.delete_line();
    }

    fn restore(&mut self) {
        self.buffer.restore();
    }

    fn move_right(&mut self) {
        self.buffer.move_right();
    }

    fn move_right_word(&mut self) {
        self.buffer.move_right_word();
    }

    fn move_line_end(&mut self) {
        self.buffer.move_line_end();
    }

    fn move_up_visual(&mut self) {
        let width = self.content_width.max(1);
        let wrapped_ranges =
            self.wrap_cache
                .input_ranges(self.buffer.text_revision(), self.buffer.text(), width);
        self.buffer.move_up_visual_with_lines(wrapped_ranges);
    }

    fn move_down_visual(&mut self) {
        let width = self.content_width.max(1);
        let wrapped_ranges =
            self.wrap_cache
                .input_ranges(self.buffer.text_revision(), self.buffer.text(), width);
        self.buffer.move_down_visual_with_lines(wrapped_ranges);
    }

    fn move_left(&mut self) {
        self.buffer.move_left();
    }

    fn move_left_word(&mut self) {
        self.buffer.move_left_word();
    }

    fn move_line_start(&mut self) {
        self.buffer.move_line_start();
    }
}

#[derive(Debug, Default)]
pub(crate) struct InputEventHandler;

impl InputEventHandler {
    pub(crate) fn poll_and_handle(
        &mut self,
        timeout: Duration,
        editor: &mut InputEditor<'_>,
        approval_pending: bool,
        panel: &mut BottomPanelBar,
    ) -> Result<Vec<FrontendAction>> {
        if !event::poll(timeout)? {
            return Ok(Vec::new());
        }

        let mut actions = self.handle_event(event::read()?, editor, approval_pending, panel);

        while event::poll(Duration::from_millis(0))? {
            actions.extend(self.handle_event(event::read()?, editor, approval_pending, panel));
        }
        Ok(actions)
    }

    pub(crate) fn handle_event(
        &mut self,
        event: Event,
        editor: &mut InputEditor<'_>,
        approval_pending: bool,
        panel: &mut BottomPanelBar,
    ) -> Vec<FrontendAction> {
        match event {
            Event::Key(key_event) => {
                if !matches!(key_event.kind, Press | Repeat) {
                    return Vec::new();
                }
                if approval_pending && let Some(action) = map_approval_key_event(key_event) {
                    return vec![action];
                }
                // An interactive bottom panel (when present) consumes keys it owns
                // before the composer sees them.
                if panel.handle_key(ui_key(key_event))
                    != crate::presentation::InteractionResult::Ignored
                {
                    return vec![FrontendAction::RedrawOnly];
                }
                let custom_key_event = map_key_event(key_event);
                self.handle_key_event(custom_key_event, editor)
            }
            Event::Paste(text) => {
                self.handle_key_event(CustomKeyEvent::InsertPaste { text }, editor)
            }
            Event::Resize(_, _) => vec![FrontendAction::RedrawOnly],
            _ => Vec::new(),
        }
    }

    fn handle_key_event(
        &mut self,
        custom_key_event: CustomKeyEvent,
        editor: &mut InputEditor<'_>,
    ) -> Vec<FrontendAction> {
        match custom_key_event {
            CustomKeyEvent::Send => {
                if editor.text().is_empty() {
                    return Vec::new();
                }
                let text = editor.submission_text();
                editor.clear();
                vec![FrontendAction::SubmitTurn { text }]
            }
            CustomKeyEvent::CtrlC => {
                if !editor.text().is_empty() {
                    editor.clear();
                    return vec![FrontendAction::RedrawOnly];
                }
                vec![FrontendAction::RequestExit]
            }
            CustomKeyEvent::InsertChar { char } => {
                editor.insert_char(char);
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::InsertPaste { text } => {
                editor.insert_paste(&text);
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::InsertNewline => {
                editor.insert_char('\n');
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::DeleteChar => {
                editor.delete_char();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::DeleteWord => {
                editor.delete_word();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::DeleteLine => {
                editor.delete_line();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::Restore => {
                editor.restore();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveRightOne => {
                editor.move_right();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveRightWord => {
                editor.move_right_word();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveLineEnd => {
                editor.move_line_end();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveUpOne => {
                editor.move_up_visual();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveDownOne => {
                editor.move_down_visual();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveLeftOne => {
                editor.move_left();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveLeftWord => {
                editor.move_left_word();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::MoveLineStart => {
                editor.move_line_start();
                vec![FrontendAction::RedrawOnly]
            }
            CustomKeyEvent::CycleReasoningEffort => vec![FrontendAction::CycleReasoningEffort],
            CustomKeyEvent::Interrupt => vec![FrontendAction::InterruptActiveTurn],
            CustomKeyEvent::None => Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct InputBuffer {
    text: String,
    text_revision: u64,
    cursor: usize,
    preferred_col: Option<usize>,
    kill_buffer: String,
    large_pastes: Vec<LargePaste>,
    next_paste_id: u64,
}

#[derive(Debug, Clone)]
struct LargePaste {
    marker: String,
    text: String,
}

impl InputBuffer {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn cursor_bytes(&self) -> usize {
        self.cursor
    }

    pub(crate) fn text_revision(&self) -> u64 {
        self.text_revision
    }

    fn mark_text_changed(&mut self) {
        self.text_revision = self.text_revision.wrapping_add(1);
    }

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

    pub(crate) fn insert_str(&mut self, input: &str) {
        self.text.insert_str(self.cursor, input);
        self.set_cursor(self.cursor + input.len());
        self.mark_text_changed();
        self.preferred_col = None;
    }

    pub(crate) fn insert_paste(&mut self, input: &str) {
        let normalized = normalize_paste(input);
        if !is_large_paste(&normalized) {
            self.insert_str(&normalized);
            return;
        }

        let marker = self.next_large_paste_marker(normalized.chars().count());
        self.insert_str(&marker);
        self.large_pastes.push(LargePaste {
            marker,
            text: normalized,
        });
    }

    pub(crate) fn submission_text(&self) -> String {
        let mut text = self.text.clone();
        for paste in &self.large_pastes {
            if text.contains(&paste.marker) {
                text = text.replace(&paste.marker, &paste.text);
            }
        }
        text
    }

    fn next_large_paste_marker(&mut self, char_count: usize) -> String {
        self.next_paste_id = self.next_paste_id.wrapping_add(1).max(1);
        let base = format!("[Pasted Content {char_count} chars]");
        if !self.text.contains(&base) && !self.large_pastes.iter().any(|paste| paste.marker == base)
        {
            return base;
        }

        format!(
            "[Pasted Content {char_count} chars #{}]",
            self.next_paste_id
        )
    }

    pub(crate) fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.set_cursor(self.cursor + c.len_utf8());
        self.mark_text_changed();
        self.preferred_col = None;
    }

    pub(crate) fn delete_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_boundary(self.cursor);
        self.text.drain(prev..self.cursor);
        self.set_cursor(prev);
        self.mark_text_changed();
        self.preferred_col = None;
    }

    pub(crate) fn delete_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_word_start(self.cursor);
        self.text.drain(prev..self.cursor);
        self.set_cursor(prev);
        self.mark_text_changed();
        self.preferred_col = None;
    }

    pub(crate) fn delete_line(&mut self) {
        let start = self.line_start(self.cursor);
        if self.cursor == start {
            if self.cursor == 0 {
                return;
            }
            let prev = self.prev_line_end(self.cursor);
            self.text.drain(prev..self.cursor);
            self.set_cursor(prev);
            self.mark_text_changed();
            self.preferred_col = None;
            return;
        }

        self.kill_buffer = self.text[start..self.cursor].to_string();
        self.text.drain(start..self.cursor);
        self.set_cursor(start);
        self.mark_text_changed();
        self.preferred_col = None;
    }

    pub(crate) fn restore(&mut self) {
        if self.kill_buffer.is_empty() {
            return;
        }
        let kill = std::mem::take(&mut self.kill_buffer);
        self.insert_str(&kill);
        self.kill_buffer = kill;
    }

    pub(crate) fn clear(&mut self) {
        if !self.text.is_empty() {
            self.text.clear();
            self.mark_text_changed();
        }
        self.large_pastes.clear();
        self.set_cursor(0);
        self.preferred_col = None;
    }

    pub(crate) fn move_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        self.set_cursor(self.next_boundary(self.cursor));
        self.preferred_col = None;
    }

    pub(crate) fn move_right_word(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        self.set_cursor(self.next_word_start(self.cursor));
        self.preferred_col = None;
    }

    pub(crate) fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.set_cursor(self.prev_boundary(self.cursor));
        self.preferred_col = None;
    }

    pub(crate) fn move_left_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.set_cursor(self.prev_word_start(self.cursor));
        self.preferred_col = None;
    }

    pub(crate) fn move_line_start(&mut self) {
        self.set_cursor(self.line_start(self.cursor));
        self.preferred_col = None;
    }

    pub(crate) fn move_line_end(&mut self) {
        self.set_cursor(self.line_end(self.cursor));
        self.preferred_col = None;
    }

    pub(crate) fn move_up_visual_with_lines(&mut self, lines: &[Range<usize>]) {
        let Some(line_idx) = wrapped_line_index_by_start(lines, self.cursor) else {
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

        let previous = &lines[line_idx - 1];
        self.set_cursor(cursor_for_display_col(
            &self.text,
            previous.start,
            previous.end,
            target_col,
        ));
    }

    pub(crate) fn move_down_visual_with_lines(&mut self, lines: &[Range<usize>]) {
        let Some(line_idx) = wrapped_line_index_by_start(lines, self.cursor) else {
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
        self.text[..pos]
            .rfind('\n')
            .map_or(0, |idx| idx + '\n'.len_utf8())
    }

    fn line_end(&self, pos: usize) -> usize {
        let pos = self.clamp_pos_to_char_boundary(pos);
        let end = self.text[pos..]
            .find('\n')
            .map_or(self.text.len(), |idx| pos + idx);

        if end > 0 && self.text.as_bytes().get(end - 1) == Some(&b'\r') {
            end - 1
        } else {
            end
        }
    }

    fn prev_line_end(&self, pos: usize) -> usize {
        let pos = self.clamp_pos_to_char_boundary(pos);
        let Some(newline_idx) = self.text[..pos].rfind('\n') else {
            return pos;
        };

        if newline_idx > 0 && self.text.as_bytes().get(newline_idx - 1) == Some(&b'\r') {
            newline_idx - 1
        } else {
            newline_idx
        }
    }

    fn is_word_separator(ch: char) -> bool {
        ch.is_whitespace() || WORD_SEPARATORS.contains(ch)
    }

    fn prev_word_start(&self, mut pos: usize) -> usize {
        while pos > 0 {
            let prev = self.prev_boundary(pos);
            let ch = self.text[prev..pos]
                .chars()
                .next()
                .expect("previous grapheme should contain one char");
            if !ch.is_whitespace() {
                break;
            }
            pos = prev;
        }

        if pos == 0 {
            return 0;
        }

        let initial_prev = self.prev_boundary(pos);
        let initial = self.text[initial_prev..pos]
            .chars()
            .next()
            .expect("previous grapheme should contain one char");
        let target_is_separator = Self::is_word_separator(initial) && !initial.is_whitespace();

        while pos > 0 {
            let prev = self.prev_boundary(pos);
            let ch = self.text[prev..pos]
                .chars()
                .next()
                .expect("previous grapheme should contain one char");
            if ch.is_whitespace() {
                break;
            }

            let ch_is_separator = Self::is_word_separator(ch) && !ch.is_whitespace();
            if ch_is_separator != target_is_separator {
                break;
            }

            pos = prev;
        }
        pos
    }

    fn next_word_start(&self, mut pos: usize) -> usize {
        if pos >= self.text.len() {
            return self.text.len();
        }

        let next = self.next_boundary(pos);
        let first = self.text[pos..next]
            .chars()
            .next()
            .expect("next grapheme should contain one char");

        if first.is_whitespace() {
            while pos < self.text.len() {
                let next = self.next_boundary(pos);
                let ch = self.text[pos..next]
                    .chars()
                    .next()
                    .expect("next grapheme should contain one char");
                if !ch.is_whitespace() {
                    break;
                }
                pos = next;
            }
            return pos;
        }

        let target_is_separator = Self::is_word_separator(first) && !first.is_whitespace();
        while pos < self.text.len() {
            let next = self.next_boundary(pos);
            let ch = self.text[pos..next]
                .chars()
                .next()
                .expect("next grapheme should contain one char");
            if ch.is_whitespace() {
                break;
            }

            let ch_is_separator = Self::is_word_separator(ch) && !ch.is_whitespace();
            if ch_is_separator != target_is_separator {
                break;
            }

            pos = next;
        }

        while pos < self.text.len() {
            let next = self.next_boundary(pos);
            let ch = self.text[pos..next]
                .chars()
                .next()
                .expect("next grapheme should contain one char");
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

fn normalize_paste(input: &str) -> String {
    input
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
}

fn is_large_paste(text: &str) -> bool {
    text.chars().count() > LARGE_PASTE_CHAR_THRESHOLD
        || text.split('\n').count() > LARGE_PASTE_LINE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_paste_inserts_visible_text_directly() {
        let mut input = InputBuffer::default();

        input.insert_paste("hello\tworld");

        assert_eq!(input.text(), "hello    world");
        assert_eq!(input.submission_text(), "hello    world");
    }

    #[test]
    fn large_paste_uses_placeholder_but_submits_full_text() {
        let mut input = InputBuffer::default();
        let paste = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 1);

        input.insert_paste(&paste);

        assert_eq!(input.text(), "[Pasted Content 1001 chars]");
        assert_eq!(input.submission_text(), paste);
    }

    #[test]
    fn multiline_large_paste_uses_placeholder() {
        let mut input = InputBuffer::default();
        let paste = (0..=LARGE_PASTE_LINE_THRESHOLD)
            .map(|idx| format!("line{idx}"))
            .collect::<Vec<_>>()
            .join("\n");

        input.insert_paste(&paste);

        assert_eq!(input.text(), "[Pasted Content 66 chars]");
        assert_eq!(input.submission_text(), paste);
    }

    #[test]
    fn duplicate_large_paste_markers_are_unique_and_expand() {
        let mut input = InputBuffer::default();
        let paste = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 1);

        input.insert_paste(&paste);
        input.insert_char(' ');
        input.insert_paste(&paste);

        assert_eq!(
            input.text(),
            "[Pasted Content 1001 chars] [Pasted Content 1001 chars #2]"
        );
        assert_eq!(input.submission_text(), format!("{paste} {paste}"));
    }

    #[test]
    fn edited_placeholder_does_not_expand_stale_paste() {
        let mut input = InputBuffer::default();
        let paste = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 1);

        input.insert_paste(&paste);
        input.delete_char();

        assert_eq!(input.submission_text(), "[Pasted Content 1001 chars");
    }

    #[test]
    fn esc_maps_to_interrupt_in_normal_mode() {
        assert!(matches!(
            map_key_event(KeyEvent::new(Esc, KeyModifiers::NONE)),
            CustomKeyEvent::Interrupt
        ));
        // In approval mode the controller routes Esc to reject-first via the
        // higher-priority approval mapping.
        assert!(matches!(
            map_approval_key_event(KeyEvent::new(Esc, KeyModifiers::NONE)),
            Some(FrontendAction::RejectPendingTool)
        ));
    }
}
