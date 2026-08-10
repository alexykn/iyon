//! Generic semantic streaming sources and local stream panes.
//!
//! Native history is deliberately outside this module. A [`StreamPane`] owns
//! local semantic scrolling without relinquishing resident content.

mod append;
mod compile;
mod coord;
mod model;
pub(crate) mod node;
mod projected;
mod resident;
mod snapshot;
mod source;
mod transfer;
mod validate;
mod viewport;

mod pane;

#[cfg(test)]
mod tests;

pub(crate) use compile::{
    CompiledStream, StreamAtomicId, StreamRowAnchor, StreamRowTransfer, compile_stream,
};
pub use coord::{StreamOffset, StreamRange, StreamRevision};
pub use model::StreamModelError as StreamError;
pub(crate) use model::{StreamModel, StreamModelError};
pub(crate) use node::{StreamNode, StreamView};
pub use pane::StreamPane;
pub(crate) use projected::{
    ExactTerminator, ProjectedTextLayout, ProjectedTextRun, projected_atoms,
};
pub use projected::{ProjectedText, ProjectedTextBuilder};
pub use snapshot::{StreamSnapshot, StreamSnapshotBuilder};
pub use source::StreamingSource;
pub(crate) use transfer::{
    FrozenPhysicalRows, StreamPartialTransfer, StreamTransferPayload, plan_stream_transfer,
};
pub use validate::{ProjectedValidationError, StreamValidationError};
pub(crate) use viewport::{StreamRowIndex, build_index_from, window_view};

#[cfg(test)]
pub(crate) use viewport::{build_index_call_count, reset_build_index_call_count};
