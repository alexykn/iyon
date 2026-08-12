//! Generic synchronous stream source contract.

use super::{coord::StreamOffset, snapshot::StreamSnapshot};

/// Trait for synchronous append-only streaming content.
///
/// A source chooses one monotonic root-coordinate convention for its
/// lifetime. Text sources use UTF-8 byte offsets; record or event sources may
/// use ordinals. The trait intentionally has no `Send`, `Sync`, `Debug`,
/// `Clone`, async, or ticking requirement.
pub trait StreamingSource: 'static {
    /// Width-independent semantic presentation of the current unacknowledged stream state.
    fn snapshot(&self) -> StreamSnapshot;

    /// Retention hint: source semantics before this offset no longer need to be
    /// reconstructed solely from mutable source state. It is not consumer ownership.
    /// Retention hint only; this does not transfer ownership to terminal history.
    fn compact_before(&mut self, _offset: StreamOffset) {}

    /// Seals the stream against future mutations, stabilizing the final semantic content in-place.
    fn seal(&mut self);

    /// Whether the stream has been sealed.
    fn is_sealed(&self) -> bool;
}
