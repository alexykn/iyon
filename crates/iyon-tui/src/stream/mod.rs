//! Private generic stream infrastructure.
//!
//! This subsystem owns source coordinates, semantic projection, snapshots,
//! resident semantic ownership, and width-dependent backend-neutral lowering.
//! Conversation/native-history ownership lives in `transcript::hosted_stream`.

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

#[cfg(test)]
mod tests;

pub(crate) use append::append_only_text_stable_frontier;
pub(crate) use compile::{CompiledStream, StreamAtomicId, StreamRowTransfer, compile_stream};
pub(crate) use coord::{StreamOffset, StreamRange, StreamRevision};
pub(crate) use model::{StreamModel, StreamModelError};
pub(crate) use node::{StreamNode, StreamProvenance, StreamView};
pub(crate) use projected::{
    ExactTerminator, ProjectedAtom, ProjectedText, ProjectedTextLayout, ProjectedTextRun,
    projected_atoms, projected_checkpoint_is_legal, slice_projected_text,
};
pub(crate) use resident::ResidentPrefix;
pub(crate) use snapshot::StreamSnapshot;
pub(crate) use source::StreamingSource;
pub(crate) use validate::{ProjectedValidationError, StreamValidationError};
