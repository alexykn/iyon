use super::super::{StreamRowAnchor, StreamRowIndex};
use super::{StreamPaneMode, StreamViewportAnchor, anchor_matches};
pub(super) type StreamPaneRowIndex = StreamRowIndex;

pub(super) fn top_index(
    mode: &StreamPaneMode,
    index: &StreamPaneRowIndex,
    viewport: usize,
) -> usize {
    match mode {
        StreamPaneMode::FollowEnd => index.anchors.len().saturating_sub(viewport),
        StreamPaneMode::Detached(anchor) => index
            .anchors
            .iter()
            .position(|candidate| anchor_matches(anchor, candidate))
            .unwrap_or_else(|| nearest_anchor(index, anchor)),
    }
}

pub(super) fn nearest_anchor(index: &StreamPaneRowIndex, anchor: &StreamViewportAnchor) -> usize {
    let target = match anchor {
        StreamViewportAnchor::Checkpoint(offset) => *offset,
        StreamViewportAnchor::Atomic { range, .. } => range.start(),
    };
    index
        .anchors
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let offset = match candidate {
                StreamRowAnchor::Checkpoint(offset) => *offset,
                StreamRowAnchor::Atomic { range, .. } => range.start(),
            };
            (offset <= target).then_some((index, offset))
        })
        .max_by_key(|(_, offset)| *offset)
        .map_or(0, |(index, _)| index)
}
