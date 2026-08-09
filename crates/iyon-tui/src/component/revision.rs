/// Monotonic revision of a retained component entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ComponentRevision(u64);

impl ComponentRevision {
    pub(crate) const fn value(self) -> u64 {
        self.0
    }

    pub(crate) fn increment(self) -> Self {
        Self(self.0.checked_add(1).expect("component revision exhausted"))
    }
}
