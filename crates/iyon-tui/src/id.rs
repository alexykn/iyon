use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn next_nonzero_id(counter: &AtomicU64, exhausted: &'static str) -> u64 {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("{exhausted}"))
}
