use super::super::StreamRange;
use super::anchor::anchor_end;
use super::index::top_index;
use super::{StreamPane, StreamRowAnchor, StreamingSource};
use crate::View;

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
        let anchor = &index.anchors[top];
        let (start, skip) = match anchor {
            StreamRowAnchor::Checkpoint(offset) => (*offset, 0),
            StreamRowAnchor::Atomic { range, row } => (range.start(), *row as u16),
        };
        let end = index
            .anchors
            .get(top.saturating_add(usize::from(size.height)))
            .map_or(self.model.snapshot().source_end, anchor_end);
        let view = self
            .model
            .semantic_slice(StreamRange::new(start, end))
            .expect("compiled stream row anchors must be legal semantic cuts")
            .into_static_view();
        View::row_viewport(view, skip)
    }
}
