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
    pub(crate) anchors: Vec<StreamRowAnchor>,
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
    crate::perf::add(
        crate::perf::Counter::StreamRowsReindexed,
        compiled.rows.len() as u64,
    );
    StreamRowIndex {
        revision: snapshot.revision,
        width,
        anchors: compiled.rows.into_iter().map(|row| row.anchor).collect(),
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
