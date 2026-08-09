use super::super::{StreamModel, StreamRevision, StreamRowAnchor, StreamingSource, compile_stream};

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
    let snapshot = model.snapshot();
    let compiled = compile_stream(&model.semantic_view(), width, snapshot.stable_through);
    StreamRowIndex {
        revision: snapshot.revision,
        width,
        anchors: compiled.rows.into_iter().map(|row| row.anchor).collect(),
    }
}
