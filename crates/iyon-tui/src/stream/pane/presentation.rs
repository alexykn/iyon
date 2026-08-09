use super::index::top_index;
use super::{StreamPane, StreamingSource};
use crate::View;
use crate::stream::window_view;

impl<S: StreamingSource> StreamPane<S> {
    pub(super) fn render_view(&self) -> View {
        let Some(size) = self.layout_size else {
            return self.model.semantic_view().into_static_view();
        };
        if size.width == 0 || size.height == 0 {
            return View::spacer(0).fill_width().fill_height();
        }
        let Some(index) = self.row_index.as_ref() else {
            return View::spacer(0).fill_width().fill_height();
        };
        if index.anchors.is_empty() {
            return View::spacer(0).fill_width().fill_height();
        }
        let viewport = usize::from(size.height);
        let top = top_index(&self.mode, index, viewport).min(index.anchors.len() - 1);
        window_view(&self.model, index, top, size.height)
    }
}
