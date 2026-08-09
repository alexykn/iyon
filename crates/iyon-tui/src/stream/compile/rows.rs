//! Backend-neutral compiled stream rows and semantic metadata.

use std::ops::Deref;

use crate::physical::PhysicalRow;

use super::super::{StreamOffset, StreamRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct StreamAtomicId(pub(crate) usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamRowAnchor {
    /// A reproducible semantic checkpoint at the beginning of a physical row.
    Checkpoint(StreamOffset),
    /// A physical subrow inside one indivisible semantic node.
    Atomic { range: StreamRange, row: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamRowTransfer {
    /// The remaining semantic projection is reproducible from this checkpoint.
    Checkpoint(StreamOffset),
    /// Rows belong to one indivisible semantic atomic group.
    Atomic {
        group: StreamAtomicId,
        source_end: StreamOffset,
    },
    /// Transfer is blocked by instability, an earlier block, or impossible fit.
    Blocked,
}

/// One compiled physical row with independent viewport and transfer metadata.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledStreamRow {
    pub(crate) physical: PhysicalRow,
    pub(crate) anchor: StreamRowAnchor,
    pub(crate) transfer: StreamRowTransfer,
}

impl Deref for CompiledStreamRow {
    type Target = PhysicalRow;

    fn deref(&self) -> &Self::Target {
        &self.physical
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledStream {
    pub(crate) rows: Vec<CompiledStreamRow>,
    pub(crate) transferable_prefix_rows: usize,
}
