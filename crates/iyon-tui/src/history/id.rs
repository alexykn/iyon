//! Opaque identity for ordered semantic history units.

use std::{
    fmt,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_HISTORY_UNIT_ID: AtomicU64 = AtomicU64::new(1);

/// Stable identity for one unit in one process's generic History namespace.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct HistoryUnitId(NonZeroU64);

impl HistoryUnitId {
    pub(crate) fn allocate() -> Self {
        let value = NEXT_HISTORY_UNIT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .unwrap_or_else(|_| panic!("history unit id exhausted"));
        Self(NonZeroU64::new(value).expect("history unit id must be nonzero"))
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
