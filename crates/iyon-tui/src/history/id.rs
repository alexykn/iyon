//! Opaque identity for ordered semantic history units.

use std::{fmt, num::NonZeroU64, sync::atomic::AtomicU64};

use crate::id::next_nonzero_id;

static NEXT_HISTORY_UNIT_ID: AtomicU64 = AtomicU64::new(1);

/// Stable identity for one unit in one process's generic History namespace.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HistoryUnitId(NonZeroU64);

impl HistoryUnitId {
    pub(crate) fn allocate() -> Self {
        Self(next_nonzero_id(
            &NEXT_HISTORY_UNIT_ID,
            "history unit id exhausted",
        ))
    }

    pub(crate) const fn value(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Debug for HistoryUnitId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("HistoryUnitId")
            .field(&self.value())
            .finish()
    }
}
