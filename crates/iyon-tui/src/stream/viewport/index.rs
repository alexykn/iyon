use super::super::{StreamModel, StreamRevision, StreamRowAnchor, StreamingSource, compile_stream};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static BUILD_INDEX_CALLS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StreamRowIndex {
    pub(crate) revision: StreamRevision,
    pub(crate) width: u16,
    /// Source offset this index starts at (its row 0 anchors this offset).
    pub(crate) indexed_from: super::super::StreamOffset,
    pub(crate) anchors: Vec<StreamRowAnchor>,
    /// Hard-line start of every row, parallel to `anchors`.
    pub(crate) hard_line_starts: Vec<super::super::StreamOffset>,
}

impl StreamRowIndex {
    /// Drops anchors whose checkpoint is strictly before `offset`, adjusting the
    /// indexed-from base. Used when a native sink releases resident prefix rows.
    pub(crate) fn retain_from(&mut self, offset: super::super::StreamOffset) {
        let first = self
            .anchors
            .iter()
            .position(|anchor| anchor_offset(anchor) >= offset)
            .unwrap_or(self.anchors.len());
        self.anchors.drain(..first);
        self.hard_line_starts.drain(..first);
        self.indexed_from = self.indexed_from.max(offset);
    }
}

pub(crate) fn build_index<S: StreamingSource>(
    model: &StreamModel<S>,
    width: u16,
) -> StreamRowIndex {
    build_index_from(model, super::super::StreamOffset::ZERO, width)
}

pub(crate) fn build_index_from<S: StreamingSource>(
    model: &StreamModel<S>,
    start: super::super::StreamOffset,
    width: u16,
) -> StreamRowIndex {
    #[cfg(test)]
    BUILD_INDEX_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));
    crate::perf::set(
        crate::perf::Counter::StreamSemanticRestartOffset,
        start.as_u64(),
    );
    crate::perf::set(
        crate::perf::Counter::StreamVisualRestartOffset,
        start.as_u64(),
    );

    let snapshot = model.snapshot();
    let compiled = compile_stream(
        &model.semantic_view_from(start),
        width,
        snapshot.stable_through,
    );
    let anchors = compiled
        .rows
        .into_iter()
        .map(|row| row.anchor)
        .collect::<Vec<_>>();
    let hard_line_starts = model.hard_line_starts_for(&anchors, start);
    crate::perf::add(
        crate::perf::Counter::StreamRowsReindexed,
        anchors.len() as u64,
    );
    StreamRowIndex {
        revision: snapshot.revision,
        width,
        indexed_from: start,
        anchors,
        hard_line_starts,
    }
}

/// Incrementally rebuilds an index whose semantic content changed at
/// `semantic_changed_from` by reflowing only from the whole hard line that
/// first contains that offset. Rows before that hard line are carried forward
/// verbatim; the suffix is compiled fresh.
pub(crate) fn reindex_from<S: StreamingSource>(
    model: &StreamModel<S>,
    previous: &StreamRowIndex,
    start: super::super::StreamOffset,
    semantic_changed_from: super::super::StreamOffset,
    width: u16,
) -> StreamRowIndex {
    if previous.width != width || previous.indexed_from != start {
        return build_index_from(model, start, width);
    }
    let all_atomic = previous
        .anchors
        .iter()
        .all(|anchor| matches!(anchor, StreamRowAnchor::Atomic { .. }));
    let last_atomic_is_stable = all_atomic
        && previous.anchors.last().is_some_and(|anchor| {
            matches!(
                anchor,
                StreamRowAnchor::Atomic { range, .. }
                    if range.end <= semantic_changed_from
            )
        });
    let mut visual_restart = if last_atomic_is_stable {
        semantic_changed_from
    } else {
        previous
            .hard_line_starts
            .iter()
            .copied()
            .take_while(|line_start| *line_start <= semantic_changed_from)
            .last()
            .unwrap_or(start)
    };
    if visual_restart < start {
        visual_restart = start;
    }
    crate::perf::set(
        crate::perf::Counter::StreamSemanticRestartOffset,
        semantic_changed_from.as_u64(),
    );
    crate::perf::set(
        crate::perf::Counter::StreamVisualRestartOffset,
        visual_restart.as_u64(),
    );

    let visual_suffix = model.semantic_view_from(visual_restart);
    let mut retained = Vec::with_capacity(previous.anchors.len());
    for (anchor, hard_line_start) in previous
        .anchors
        .iter()
        .zip(previous.hard_line_starts.iter())
    {
        if *hard_line_start < visual_restart {
            retained.push(anchor.clone());
        } else {
            break;
        }
    }
    let retained_len = retained.len();

    let compiled = compile_stream(&visual_suffix, width, model.snapshot().stable_through);
    retained.extend(compiled.rows.into_iter().map(|row| row.anchor));
    let mut hard_line_starts = previous.hard_line_starts[..retained_len].to_vec();
    hard_line_starts.extend(model.hard_line_starts_for_from(
        &retained[retained_len..],
        start,
        visual_restart,
    ));

    crate::perf::add(
        crate::perf::Counter::StreamStableRowsReused,
        retained_len as u64,
    );
    crate::perf::add(
        crate::perf::Counter::StreamRowsReindexed,
        retained.len().saturating_sub(retained_len) as u64,
    );
    StreamRowIndex {
        revision: model.snapshot().revision,
        width,
        indexed_from: start,
        anchors: retained,
        hard_line_starts,
    }
}

fn anchor_offset(anchor: &StreamRowAnchor) -> super::super::StreamOffset {
    match anchor {
        StreamRowAnchor::Checkpoint(offset) => *offset,
        StreamRowAnchor::Atomic { range, .. } => range.start,
    }
}

#[cfg(test)]
pub(crate) fn reset_build_index_call_count() {
    BUILD_INDEX_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn build_index_call_count() -> usize {
    BUILD_INDEX_CALLS.with(Cell::get)
}
