//! Backend-neutral compiled stream rows and semantic transfer metadata.

use crate::physical::PhysicalRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct StreamAtomicId(pub(crate) usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamRowTransfer {
    /// The remaining semantic projection is reproducible from this checkpoint.
    Checkpoint(crate::stream::StreamOffset),
    /// Rows belong to one indivisible semantic atomic group.
    Atomic {
        group: StreamAtomicId,
        source_end: crate::stream::StreamOffset,
    },
    /// Transfer is blocked by instability, an earlier block, or impossible fit.
    Blocked,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledStream {
    pub(crate) rows: Vec<PhysicalRow>,
    pub(crate) transfer: Vec<StreamRowTransfer>,
    pub(crate) transferable_prefix_rows: usize,
}
