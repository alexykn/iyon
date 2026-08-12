use std::{
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) fn next_nonzero_id(counter: &AtomicU64, exhausted: &'static str) -> NonZeroU64 {
    let value = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("{exhausted}"));
    NonZeroU64::new(value).expect("ID counters start nonzero")
}
