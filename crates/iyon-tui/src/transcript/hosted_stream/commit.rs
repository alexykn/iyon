//! Transitional native-history transaction planning.

use crate::{
    physical::PhysicalRow,
    stream::{CompiledStream, StreamAtomicId, StreamOffset, StreamRowTransfer},
};

/// Renderer-owned physical rows for atomic freeze.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrozenPhysicalRows(pub(crate) Vec<PhysicalRow>);

impl FrozenPhysicalRows {
    pub(crate) fn new(rows: Vec<PhysicalRow>) -> Self {
        Self(rows)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn as_slice(&self) -> &[PhysicalRow] {
        &self.0
    }
}

/// Payload specifying the exact physical rows to write to the terminal.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommitPayload {
    /// Slice of rows from the compiled stream layout.
    Compiled { start: usize, len: usize },

    /// Slice of frozen physical rows from a prior atomic partial commit.
    Frozen { rows: FrozenPhysicalRows },
}

impl CommitPayload {
    pub(crate) fn rows_to_write(&self) -> usize {
        match self {
            Self::Compiled { len, .. } => *len,
            Self::Frozen { rows } => rows.len(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows_to_write() == 0
    }
}

/// Persistent partial-commit state across frame / resize boundaries.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamPartialCommit {
    /// Frozen physical rows of an atomic region that was partially committed to history.
    FrozenAtomic {
        group: StreamAtomicId,
        source_end: StreamOffset,
        rows: FrozenPhysicalRows,
        committed_rows: usize,
    },
}

/// Plan for promoting physical stream rows into native history.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommitPlan {
    /// Exact physical rows to write to the terminal.
    pub(crate) payload: CommitPayload,

    /// Source offset confirmed by native history after rows are successfully written.
    pub(crate) next_committed_through: StreamOffset,

    /// Next partial commit state after rows are successfully written.
    pub(crate) next_partial: Option<StreamPartialCommit>,
}

/// Central commit planner below the host.
///
/// Determines how many physical rows may be safely promoted to native history,
/// planning cursor advancement and atomic row freezing without mutating state.
pub(crate) fn plan_commit(
    compiled: &CompiledStream,
    desired_rows: usize,
    partial: Option<&StreamPartialCommit>,
    current_committed_through: StreamOffset,
) -> CommitPlan {
    // If we are currently in a FrozenAtomic partial state, we must drain the frozen rows first.
    if let Some(StreamPartialCommit::FrozenAtomic {
        group,
        source_end,
        rows,
        committed_rows,
    }) = partial
    {
        let remaining_frozen = rows.len().saturating_sub(*committed_rows);
        let to_commit = desired_rows.min(remaining_frozen);
        if to_commit == 0 {
            return CommitPlan {
                payload: CommitPayload::Frozen {
                    rows: FrozenPhysicalRows::new(Vec::new()),
                },
                next_committed_through: current_committed_through,
                next_partial: Some(partial.cloned().unwrap()),
            };
        }

        let slice = rows.as_slice()[*committed_rows..*committed_rows + to_commit].to_vec();
        let new_committed = committed_rows.saturating_add(to_commit);

        if new_committed >= rows.len() {
            return CommitPlan {
                payload: CommitPayload::Frozen {
                    rows: FrozenPhysicalRows::new(slice),
                },
                next_committed_through: *source_end,
                next_partial: None,
            };
        } else {
            return CommitPlan {
                payload: CommitPayload::Frozen {
                    rows: FrozenPhysicalRows::new(slice),
                },
                next_committed_through: current_committed_through,
                next_partial: Some(StreamPartialCommit::FrozenAtomic {
                    group: *group,
                    source_end: *source_end,
                    rows: rows.clone(),
                    committed_rows: new_committed,
                }),
            };
        }
    }

    if desired_rows == 0 || compiled.transferable_prefix_rows == 0 {
        return CommitPlan {
            payload: CommitPayload::Compiled { start: 0, len: 0 },
            next_committed_through: current_committed_through,
            next_partial: None,
        };
    }

    let mut cursor = current_committed_through;
    let mut row_idx = 0;

    // Skip rows that have already entered native history in earlier transactions.
    // Handles both individual Exact rows and entire fully-committed Atomic groups.
    while row_idx < compiled.transferable_prefix_rows {
        match &compiled.transfer[row_idx] {
            StreamRowTransfer::Checkpoint(end) if *end <= current_committed_through => {
                row_idx += 1;
            }
            StreamRowTransfer::Atomic { group, source_end }
                if *source_end <= current_committed_through =>
            {
                let committed_group = *group;
                while row_idx < compiled.transferable_prefix_rows {
                    match &compiled.transfer[row_idx] {
                        StreamRowTransfer::Atomic { group, source_end }
                            if *group == committed_group
                                && *source_end <= current_committed_through =>
                        {
                            row_idx += 1;
                        }
                        _ => break,
                    }
                }
            }
            _ => break,
        }
    }

    let uncommitted_start = row_idx;
    let mut rows_written = 0;

    while row_idx < compiled.transferable_prefix_rows && rows_written < desired_rows {
        match &compiled.transfer[row_idx] {
            StreamRowTransfer::Checkpoint(offset) => {
                cursor = *offset;
                row_idx += 1;
                rows_written += 1;
            }
            StreamRowTransfer::Atomic { group, source_end } => {
                let current_group = *group;
                let target_source_end = *source_end;
                let group_start = row_idx;
                let mut group_end = row_idx;

                while group_end < compiled.transfer.len()
                    && matches!(&compiled.transfer[group_end], StreamRowTransfer::Atomic { group: g, .. } if *g == current_group)
                {
                    group_end += 1;
                }

                let group_rows = group_end - group_start;
                let remaining_desired = desired_rows - rows_written;

                if group_rows <= remaining_desired && group_end <= compiled.transferable_prefix_rows
                {
                    cursor = target_source_end;
                    row_idx = group_end;
                    rows_written += group_rows;
                } else {
                    let take_in_group = remaining_desired.min(
                        compiled
                            .transferable_prefix_rows
                            .saturating_sub(group_start),
                    );
                    let frozen_rows = compiled.rows[group_start..group_end].to_vec();

                    return CommitPlan {
                        payload: CommitPayload::Compiled {
                            start: uncommitted_start,
                            len: rows_written + take_in_group,
                        },
                        next_committed_through: cursor,
                        next_partial: Some(StreamPartialCommit::FrozenAtomic {
                            group: current_group,
                            source_end: target_source_end,
                            rows: FrozenPhysicalRows::new(frozen_rows),
                            committed_rows: take_in_group,
                        }),
                    };
                }
            }
            StreamRowTransfer::Blocked => {
                break;
            }
        }
    }

    CommitPlan {
        payload: CommitPayload::Compiled {
            start: uncommitted_start,
            len: rows_written,
        },
        next_committed_through: cursor,
        next_partial: None,
    }
}
