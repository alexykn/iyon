//! A backend-neutral, stateful Unicode text editor component.
//!
//! [`TextInput`] owns editing mechanics and emits semantic outputs. It does
//! not know about application submission policy, terminal backends, or
//! geometry.

mod buffer;
mod command;
mod cursor;
mod edit;
mod output;
mod presentation;

#[cfg(test)]
mod tests;

use std::ops::Range;

use crate::{
    Component, ComponentCx, EventCx, InteractionResult, Output, View, geometry::Size,
    presentation::wrap::input_wrap_ranges,
};

use self::{
    buffer::TextBuffer,
    command::{TextInputCommand, command_for_key, handle_command},
    output::ChangeOutputs,
};

pub use output::TextChange;

/// Retained generic Unicode text editor state.
///
/// `TextInput` is intentionally constructed explicitly. Its output channels
/// are stable for the lifetime of the component and are not duplicated by
/// cloning, so this type does not implement [`Clone`].
pub struct TextInput {
    buffer: TextBuffer,
    multiline: bool,
    focused: bool,
    submitted: Output<String>,
    change_outputs: ChangeOutputs,
    layout_size: Option<Size>,
    scroll_row: usize,
}

impl TextInput {
    /// Creates an empty single-line input with its own submission channel.
    pub fn new() -> Self {
        Self {
            buffer: TextBuffer::new(),
            multiline: false,
            focused: false,
            submitted: Output::new(),
            change_outputs: ChangeOutputs::default(),
            layout_size: None,
            scroll_row: 0,
        }
    }

    /// Returns a copy configured for single- or multi-line editing.
    pub fn multiline(mut self, enabled: bool) -> Self {
        self.set_multiline(enabled);
        self
    }

    /// Changes the line policy without emitting a user change event.
    pub fn set_multiline(&mut self, enabled: bool) {
        if self.multiline == enabled {
            return;
        }
        self.multiline = enabled;
        self.buffer.recanonicalize(enabled);
        self.repair_scroll();
    }

    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.text().is_empty()
    }

    pub fn cursor_bytes(&self) -> usize {
        self.buffer.cursor_bytes()
    }

    /// Replaces the canonical text and moves the cursor to its end.
    pub fn set_text(&mut self, text: impl AsRef<str>) {
        self.buffer.set_text(text, self.multiline);
        self.repair_scroll();
    }

    /// Clears text and editor-local kill state without emitting a change.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.repair_scroll();
    }

    /// Returns this input's stable submission channel.
    pub fn submitted(&self) -> Output<String> {
        self.submitted
    }

    /// Registers an ordered synchronous projection of user text changes.
    pub fn output_on_change<R: 'static>(
        &mut self,
        project: impl for<'change> Fn(TextChange<'change>) -> R + 'static,
    ) -> Output<R> {
        self.change_outputs.register(project)
    }

    fn insert_text(&mut self, text: &str) -> bool {
        let changed = self.buffer.insert_text(text, self.multiline);
        if changed {
            self.repair_scroll();
        }
        changed
    }

    fn emit_change(&self, cx: &mut EventCx<'_>) {
        self.change_outputs.emit(&self.buffer, cx);
    }

    fn move_up(&mut self) -> bool {
        let changed = if let Some(size) = self.layout_size {
            let rows = input_wrap_ranges(self.text(), size.width);
            self.buffer.move_up_in_rows(&rows)
        } else {
            let rows = self.buffer.logical_rows();
            self.buffer.move_up_in_rows(&rows)
        };
        self.repair_scroll();
        changed
    }

    fn move_down(&mut self) -> bool {
        let changed = if let Some(size) = self.layout_size {
            let rows = input_wrap_ranges(self.text(), size.width);
            self.buffer.move_down_in_rows(&rows)
        } else {
            let rows = self.buffer.logical_rows();
            self.buffer.move_down_in_rows(&rows)
        };
        self.repair_scroll();
        changed
    }

    fn can_execute(&self, command: TextInputCommand) -> bool {
        match command {
            TextInputCommand::Insert(_) | TextInputCommand::InsertNewline => true,
            TextInputCommand::Submit => true,
            TextInputCommand::Backspace => self.cursor_bytes() > 0,
            TextInputCommand::Delete => self.cursor_bytes() < self.text().len(),
            TextInputCommand::DeleteWordBackward => self.cursor_bytes() > 0,
            TextInputCommand::DeleteWordForward => self.cursor_bytes() < self.text().len(),
            TextInputCommand::KillToLineStart => self.cursor_bytes() > 0,
            TextInputCommand::Yank => self.buffer.has_kill_buffer(),
            TextInputCommand::MoveLeft => self.cursor_bytes() > 0,
            TextInputCommand::MoveRight => self.cursor_bytes() < self.text().len(),
            TextInputCommand::MoveWordLeft => self.cursor_bytes() > 0,
            TextInputCommand::MoveWordRight => self.cursor_bytes() < self.text().len(),
            TextInputCommand::MoveLineStart => self.cursor_bytes() != self.line_start(),
            TextInputCommand::MoveLineEnd => self.cursor_bytes() != self.line_end(),
            TextInputCommand::MoveUp | TextInputCommand::MoveDown => true,
        }
    }

    fn line_start(&self) -> usize {
        self.text()[..self.cursor_bytes()]
            .rfind('\n')
            .map_or(0, |index| index + 1)
    }

    fn line_end(&self) -> usize {
        self.text()[self.cursor_bytes()..]
            .find('\n')
            .map_or(self.text().len(), |index| self.cursor_bytes() + index)
    }

    fn focus_changed(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn handle_paste(&mut self, text: &str, cx: &mut EventCx<'_>) -> InteractionResult {
        if !self.insert_text(text) {
            return InteractionResult::Ignored;
        }
        self.emit_change(cx);
        InteractionResult::Consumed
    }

    fn command_for_key(input: &Self, key: crate::KeyStroke) -> Option<TextInputCommand> {
        command_for_key(input, key)
    }

    fn handle_command(
        input: &mut Self,
        command: TextInputCommand,
        cx: &mut EventCx<'_>,
    ) -> InteractionResult {
        let result = handle_command(input, command, cx);
        input.repair_scroll();
        result
    }

    fn layout_changed(input: &mut Self, size: Size) {
        input.layout_size = Some(size);
        input.repair_scroll();
    }

    fn repair_scroll(&mut self) {
        let Some(size) = self.layout_size else {
            return;
        };
        if size.width == 0 || size.height == 0 {
            self.scroll_row = 0;
            return;
        }
        let ranges = input_wrap_ranges(self.text(), size.width);
        let visible = usize::from(size.height).max(1);
        let max_scroll = ranges.len().saturating_sub(visible);
        let cursor = self.cursor_bytes().min(self.text().len());
        let cursor_row = cursor::wrapped_line_index_by_start(&ranges, cursor).unwrap_or(0);
        let mut scroll = self.scroll_row.min(max_scroll);
        if cursor_row < scroll {
            scroll = cursor_row;
        }
        if cursor_row >= scroll.saturating_add(visible) {
            scroll = cursor_row + 1 - visible;
        }
        self.scroll_row = scroll.min(max_scroll);
    }

    fn focus_changed_callback(input: &mut Self, focused: bool) {
        input.focus_changed(focused);
    }

    fn paste_callback(input: &mut Self, text: &str, cx: &mut EventCx<'_>) -> InteractionResult {
        input.handle_paste(text, cx)
    }

    #[cfg(test)]
    pub(crate) fn set_cursor_for_test(&mut self, cursor: usize) {
        self.buffer.set_cursor(cursor);
    }

    #[cfg(test)]
    pub(crate) fn move_up_in_rows_for_test(&mut self, rows: &[Range<usize>]) -> bool {
        self.buffer.move_up_in_rows(rows)
    }

    #[cfg(test)]
    pub(crate) fn move_down_in_rows_for_test(&mut self, rows: &[Range<usize>]) -> bool {
        self.buffer.move_down_in_rows(rows)
    }

    #[cfg(test)]
    pub(crate) fn set_focused_for_test(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl Component for TextInput {
    fn view(&self) -> View {
        self.semantic_view()
    }

    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        cx.focusable();
        cx.on_focus_changed(Self::focus_changed_callback);
        cx.key_commands(Self::command_for_key, Self::handle_command);
        cx.on_paste(Self::paste_callback);
        cx.on_layout_changed(Self::layout_changed);
    }
}
