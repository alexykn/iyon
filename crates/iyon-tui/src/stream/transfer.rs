//! Backend-neutral native-history transfer planning for semantic Streams.

use crate::{
    physical::PhysicalRow,
    stream::{CompiledStream, StreamAtomicId, StreamOffset, StreamRowTransfer},
};

/// Physical rows retained when an Atomic stream node is acknowledged partially.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrozenPhysicalRows(pub(crate) Vec<PhysicalRow>);

impl FrozenPhysicalRows {
    pub(crate) fn new(rows: Vec<PhysicalRow>) -> Self {
        Self(rows)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn as_slice(&self) -> &[PhysicalRow] {
        &self.0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Exact physical rows selected by the stream transfer planner.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamTransferPayload {
    Compiled { start: usize, len: usize },
    Frozen { rows: FrozenPhysicalRows },
}

impl StreamTransferPayload {
    pub(crate) fn rows_to_write(&self) -> usize {
        match self {
            Self::Compiled { len, .. } => *len,
            Self::Frozen { rows } => rows.len(),
        }
    }
}

/// Persistent partial-Atomic state across transfer calls and resizes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StreamPartialCommit {
    FrozenAtomic {
        group: StreamAtomicId,
        source_end: StreamOffset,
        rows: FrozenPhysicalRows,
        committed_rows: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StreamTransferPlan {
    pub(crate) payload: StreamTransferPayload,
    pub(crate) next_committed_through: StreamOffset,
    pub(crate) next_partial: Option<StreamPartialCommit>,
}

/// Pure stream transfer planner. It never mutates the StreamModel or frontier.
pub(crate) fn plan_stream_transfer(
    compiled: &CompiledStream,
    desired_rows: usize,
    partial: Option<&StreamPartialCommit>,
    current_committed_through: StreamOffset,
) -> StreamTransferPlan {
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
            return StreamTransferPlan {
                payload: StreamTransferPayload::Frozen {
                    rows: FrozenPhysicalRows::new(Vec::new()),
                },
                next_committed_through: current_committed_through,
                next_partial: Some(partial.cloned().unwrap()),
            };
        }

        let slice = rows.as_slice()[*committed_rows..*committed_rows + to_commit].to_vec();
        let new_committed = committed_rows.saturating_add(to_commit);
        if new_committed >= rows.len() {
            return StreamTransferPlan {
                payload: StreamTransferPayload::Frozen {
                    rows: FrozenPhysicalRows::new(slice),
                },
                next_committed_through: *source_end,
                next_partial: None,
            };
        }
        return StreamTransferPlan {
            payload: StreamTransferPayload::Frozen {
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

    if desired_rows == 0 || compiled.transferable_prefix_rows == 0 {
        return empty_plan(current_committed_through);
    }

    let mut cursor = current_committed_through;
    let mut row_idx = 0;
    while row_idx < compiled.transferable_prefix_rows {
        match &compiled.rows[row_idx].transfer {
            StreamRowTransfer::Checkpoint(end) if *end <= current_committed_through => {
                row_idx += 1;
            }
            StreamRowTransfer::Atomic { group, source_end }
                if *source_end <= current_committed_through =>
            {
                let committed_group = *group;
                while row_idx < compiled.transferable_prefix_rows {
                    match &compiled.rows[row_idx].transfer {
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
        match &compiled.rows[row_idx].transfer {
            StreamRowTransfer::Checkpoint(offset) => {
                cursor = *offset;
                row_idx += 1;
                rows_written += 1;
            }
            StreamRowTransfer::Atomic { group, source_end } => {
                let group_start = row_idx;
                let mut group_end = row_idx;
                while group_end < compiled.rows.len()
                    && matches!(
                        &compiled.rows[group_end].transfer,
                        StreamRowTransfer::Atomic { group: candidate, .. }
                            if candidate == group
                    )
                {
                    group_end += 1;
                }
                let group_rows = group_end - group_start;
                let remaining = desired_rows - rows_written;
                if group_rows <= remaining && group_end <= compiled.transferable_prefix_rows {
                    cursor = *source_end;
                    row_idx = group_end;
                    rows_written += group_rows;
                    continue;
                }

                let take = remaining.min(
                    compiled
                        .transferable_prefix_rows
                        .saturating_sub(group_start),
                );
                let frozen_rows = compiled.rows[group_start..group_end]
                    .iter()
                    .map(|row| row.physical.clone())
                    .collect();
                return StreamTransferPlan {
                    payload: StreamTransferPayload::Compiled {
                        start: uncommitted_start,
                        len: rows_written + take,
                    },
                    next_committed_through: cursor,
                    next_partial: Some(StreamPartialCommit::FrozenAtomic {
                        group: *group,
                        source_end: *source_end,
                        rows: FrozenPhysicalRows::new(frozen_rows),
                        committed_rows: take,
                    }),
                };
            }
            StreamRowTransfer::Blocked => break,
        }
    }

    StreamTransferPlan {
        payload: StreamTransferPayload::Compiled {
            start: uncommitted_start,
            len: rows_written,
        },
        next_committed_through: cursor,
        next_partial: None,
    }
}

fn empty_plan(current_committed_through: StreamOffset) -> StreamTransferPlan {
    StreamTransferPlan {
        payload: StreamTransferPayload::Compiled { start: 0, len: 0 },
        next_committed_through: current_committed_through,
        next_partial: None,
    }
}
