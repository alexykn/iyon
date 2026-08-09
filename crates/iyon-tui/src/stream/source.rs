//! Generic synchronous stream source contract.

use super::{coord::StreamOffset, snapshot::StreamSnapshot};

/// Trait for append-only streaming content with explicit source coordinates and semantic stability.
pub(crate) trait StreamingSource {
    /// Width-independent semantic presentation of the current unacknowledged stream state.
    fn snapshot(&self) -> StreamSnapshot;

    /// Retention hint: source semantics before this offset no longer need to be
    /// reconstructed solely from mutable source state. It is not consumer ownership.
    fn compact_before(&mut self, _offset: StreamOffset) {}

    /// Seals the stream against future mutations, stabilizing the final semantic content in-place.
    fn seal(&mut self);

    /// Whether the stream has been sealed.
    fn is_sealed(&self) -> bool;
}
