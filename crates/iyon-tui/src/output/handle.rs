use std::{
    fmt,
    hash::Hash,
    marker::PhantomData,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_OUTPUT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct OutputId(NonZeroU64);

impl OutputId {
    fn allocate() -> Self {
        let value = NEXT_OUTPUT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .unwrap_or_else(|_| panic!("output id exhausted"));
        Self(NonZeroU64::new(value).expect("output id must be nonzero"))
    }
}

/// Opaque typed identity for a semantic output channel.
pub struct Output<T: 'static> {
    id: OutputId,
    marker: PhantomData<fn() -> T>,
}

impl<T: 'static> Output<T> {
    pub fn new() -> Self {
        Self {
            id: OutputId::allocate(),
            marker: PhantomData,
        }
    }

    pub(super) const fn id(self) -> OutputId {
        self.id
    }
}

impl<T: 'static> Copy for Output<T> {}

impl<T: 'static> Clone for Output<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> PartialEq for Output<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: 'static> Eq for Output<T> {}

impl<T: 'static> Hash for Output<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: 'static> fmt::Debug for Output<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Output").finish()
    }
}

impl fmt::Debug for OutputId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("OutputId").field(&self.0).finish()
    }
}
