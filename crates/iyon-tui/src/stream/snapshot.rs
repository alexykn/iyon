//! Width-independent stream snapshots.

use super::{
    coord::{StreamOffset, StreamRevision},
    node::StreamView,
};

/// Width-independent snapshot of the current source-owned semantic stream.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StreamSnapshot {
    pub(crate) revision: StreamRevision,
    /// Earliest source represented by this snapshot; source retention, not consumer ownership.
    pub(crate) source_base: StreamOffset,
    pub(crate) source_end: StreamOffset,
    pub(crate) stable_through: StreamOffset,
    pub(crate) view: StreamView,
}
