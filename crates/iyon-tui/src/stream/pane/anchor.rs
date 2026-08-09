use super::super::{StreamOffset, StreamRange, StreamRowAnchor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StreamPaneMode {
    FollowEnd,
    Detached(StreamViewportAnchor),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StreamViewportAnchor {
    Checkpoint(StreamOffset),
    Atomic { range: StreamRange, row: usize },
}

pub(super) fn anchor_end(anchor: &StreamRowAnchor) -> StreamOffset {
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

pub(super) fn anchor_to_viewport(anchor: &StreamRowAnchor) -> StreamViewportAnchor {
    match anchor {
        StreamRowAnchor::Checkpoint(offset) => StreamViewportAnchor::Checkpoint(*offset),
        StreamRowAnchor::Atomic { range, row } => StreamViewportAnchor::Atomic {
            range: *range,
            row: *row,
        },
    }
}

pub(super) fn anchor_matches(anchor: &StreamViewportAnchor, candidate: &StreamRowAnchor) -> bool {
    match (anchor, candidate) {
        (StreamViewportAnchor::Checkpoint(left), StreamRowAnchor::Checkpoint(right)) => {
            left == right
        }
        (
            StreamViewportAnchor::Atomic {
                range: left,
                row: left_row,
            },
            StreamRowAnchor::Atomic {
                range: right,
                row: right_row,
            },
        ) => left == right && left_row == right_row,
        _ => false,
    }
}
