//! Opaque stream coordinates.

/// Opaque monotonic coordinate within a stream's source space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct StreamOffset(pub(crate) u64);

impl StreamOffset {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(offset: u64) -> Self {
        Self(offset)
    }

    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }
}

/// Monotonic revision counter for stream snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct StreamRevision(pub(crate) u64);

impl StreamRevision {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(rev: u64) -> Self {
        Self(rev)
    }

    pub(crate) fn next(self) -> Self {
        Self(self.0.checked_add(1).expect("stream revision exhausted"))
    }
}

/// A half-open source range `[start, end)` in monotonic stream coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct StreamRange {
    pub(crate) start: StreamOffset,
    pub(crate) end: StreamOffset,
}

impl StreamRange {
    pub(crate) const fn new(start: StreamOffset, end: StreamOffset) -> Self {
        Self { start, end }
    }

    pub(crate) const fn empty(at: StreamOffset) -> Self {
        Self { start: at, end: at }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    pub(crate) fn len(&self) -> u64 {
        self.end.0.saturating_sub(self.start.0)
    }

    pub(crate) fn contains_offset(&self, offset: StreamOffset) -> bool {
        offset >= self.start && offset < self.end
    }
}
