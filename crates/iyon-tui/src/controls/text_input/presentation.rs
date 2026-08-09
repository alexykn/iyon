use crate::{IntoView, View, presentation::wrap::input_wrap_ranges};

use super::TextInput;

impl TextInput {
    pub(super) fn semantic_view(&self) -> View {
        let Some(size) = self.layout_size else {
            let text =
                View::text(self.buffer.text().to_owned()).wrap(crate::WrapMode::WordThenGrapheme);
            return if self.focused {
                text.cursor_at(self.buffer.cursor_bytes()).into_view()
            } else {
                text.into_view()
            };
        };

        if size.width == 0 || size.height == 0 {
            return View::vertical(|_| {}).fill_width().fill_height();
        }

        let ranges = input_wrap_ranges(self.buffer.text(), size.width);
        let visible = usize::from(size.height).max(1);
        let start = self.scroll_row.min(ranges.len());
        let end = (start + visible).min(ranges.len());
        let cursor = self.buffer.cursor_bytes();
        let cursor_row = crate::input::wrapped_line_index_by_start(&ranges, cursor).unwrap_or(0);
        View::vertical(|column| {
            for (row_index, range) in ranges[start..end].iter().enumerate() {
                let row_index = start + row_index;
                let row_text = self.buffer.text()[range.clone()].to_owned();
                let row = if self.focused && row_index == cursor_row {
                    View::text(row_text)
                        .no_wrap()
                        .cursor_at(cursor.saturating_sub(range.start))
                        .into_view()
                } else {
                    View::text(row_text).no_wrap().into_view()
                };
                column.child(row);
            }
        })
        .fill_width()
        .fill_height()
    }
}
