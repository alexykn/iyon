//! Generic semantic streaming sources and local stream panes.
//!
//! Native history is deliberately outside this module. A [`StreamPane`] owns
//! local semantic scrolling without relinquishing resident content.

mod append;
mod compile;
mod coord;
mod model;
mod node;
mod projected;
mod resident;
mod snapshot;
mod source;
mod validate;
mod viewport;

mod pane;

#[cfg(test)]
mod tests;

pub(crate) use append::append_only_text_stable_frontier;
pub(crate) use compile::{
    CompiledStream, StreamAtomicId, StreamRowAnchor, StreamRowTransfer, compile_stream,
};
pub use coord::{StreamOffset, StreamRange, StreamRevision};
pub use model::StreamModelError as StreamError;
pub(crate) use model::{StreamModel, StreamModelError};
pub(crate) use node::{StreamNode, StreamProvenance, StreamSliceError, StreamView};
pub use pane::StreamPane;
pub(crate) use projected::{
    ExactTerminator, ProjectedTextLayout, ProjectedTextRun, projected_atoms,
    projected_checkpoint_is_legal,
};
pub use projected::{ProjectedText, ProjectedTextBuilder};
pub(crate) use resident::ResidentPrefix;
pub use snapshot::{StreamSnapshot, StreamSnapshotBuilder};
pub use source::StreamingSource;
pub use validate::{ProjectedValidationError, StreamValidationError};
pub(crate) use viewport::{StreamRowIndex, build_index, window_view};

#[cfg(test)]
pub(crate) use viewport::{build_index_call_count, reset_build_index_call_count};
