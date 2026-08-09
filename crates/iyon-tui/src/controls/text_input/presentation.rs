use crate::{View, presentation::IntoView};

use super::TextInput;

impl TextInput {
    pub(super) fn semantic_view(&self) -> View {
        let text = View::text(self.buffer.text().to_owned()).wrap(crate::WrapMode::Grapheme);
        if self.focused {
            text.cursor_at(self.buffer.cursor_bytes()).into_view()
        } else {
            text.into_view()
        }
    }
}
