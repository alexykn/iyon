use super::super::{StreamModel, StreamRange, StreamRowAnchor, StreamingSource};
use crate::View;

fn anchor_end(anchor: &StreamRowAnchor) -> super::super::StreamOffset {
    match anchor {
        StreamRowAnchor::Checkpoint(offset) => *offset,
        StreamRowAnchor::Atomic { range, row } => {
            if *row == 0 {
                range.start()
            } else {
                range.end()
            }
        }
    }
}

pub(crate) fn window_view<S: StreamingSource>(
    model: &StreamModel<S>,
    index: &super::StreamRowIndex,
    top_row: usize,
    height: u16,
) -> View {
    if width_is_zero(index) || height == 0 || index.anchors.is_empty() {
        return View::spacer(0).fill_width().fill_height();
    }
    let top = top_row.min(index.anchors.len().saturating_sub(1));
    let anchor = &index.anchors[top];
    let (start, skip) = match anchor {
        StreamRowAnchor::Checkpoint(offset) => (*offset, 0),
        StreamRowAnchor::Atomic { range, row } => (range.start(), *row as u16),
    };
    let end = index
        .anchors
        .get(top.saturating_add(usize::from(height)))
        .map_or(model.snapshot().source_end, anchor_end);
    let view = model
        .semantic_slice(StreamRange::new(start, end))
        .expect("compiled stream row anchors must be legal semantic cuts")
        .into_static_view();
    View::row_viewport(view, skip)
}

fn width_is_zero(index: &super::StreamRowIndex) -> bool {
    index.width == 0
}
