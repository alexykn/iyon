//! Opaque stream coordinates.

/// Monotonic UTF-8 byte coordinate within a stream's source space.
///
/// Current stream sources use source byte offsets; projected mappings must
/// preserve that unit rather than substituting display-cell or grapheme
/// coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StreamOffset(pub(crate) u64);

impl StreamOffset {
    pub const ZERO: Self = Self(0);

    pub const fn new(offset: u64) -> Self {
        Self(offset)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn checked_add(self, rhs: u64) -> Option<Self> {
        match self.0.checked_add(rhs) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn saturating_add(self, rhs: u64) -> Self {
        Self(self.0.saturating_add(rhs))
    }
}

/// Monotonic revision counter for stream snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StreamRevision(pub(crate) u64);

impl StreamRevision {
    pub const ZERO: Self = Self(0);

    pub const fn new(rev: u64) -> Self {
        Self(rev)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub fn next(self) -> Self {
        self.checked_next().expect("stream revision exhausted")
    }
}

/// A half-open UTF-8 source-byte range `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StreamRange {
    pub(crate) start: StreamOffset,
    pub(crate) end: StreamOffset,
}

impl StreamRange {
    pub const fn new(start: StreamOffset, end: StreamOffset) -> Self {
        assert!(start.0 <= end.0, "stream range start exceeds end");
        Self { start, end }
    }

    pub const fn try_new(start: StreamOffset, end: StreamOffset) -> Option<Self> {
        if start.0 <= end.0 {
            Some(Self { start, end })
        } else {
            None
        }
    }

    pub const fn start(self) -> StreamOffset {
        self.start
    }

    pub const fn end(self) -> StreamOffset {
        self.end
    }

    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    pub fn len(&self) -> u64 {
        self.end.0.saturating_sub(self.start.0)
    }

    pub fn contains_offset(&self, offset: StreamOffset) -> bool {
        offset >= self.start && offset < self.end
    }
}
