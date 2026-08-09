use std::{fmt, hash::Hash, marker::PhantomData, num::NonZeroU64};

/// Opaque identity for a retained component registration.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ComponentId(NonZeroU64);

impl ComponentId {
    pub(crate) const fn value(self) -> u64 {
        self.0.get()
    }

    pub(crate) const fn from_nonzero(value: NonZeroU64) -> Self {
        Self(value)
    }
}

impl fmt::Debug for ComponentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ComponentId")
            .field(&self.value())
            .finish()
    }
}

/// Typed, non-owning identity for a component in a registry.
pub(crate) struct ComponentHandle<C> {
    id: ComponentId,
    marker: PhantomData<fn() -> C>,
}

impl<C> Copy for ComponentHandle<C> {}

impl<C> Clone for ComponentHandle<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> fmt::Debug for ComponentHandle<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentHandle")
            .field("id", &self.id)
            .finish()
    }
}

impl<C> PartialEq for ComponentHandle<C> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<C> Eq for ComponentHandle<C> {}

impl<C> Hash for ComponentHandle<C> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<C> ComponentHandle<C> {
    pub(crate) const fn id(self) -> ComponentId {
        self.id
    }

    pub(crate) const fn from_id(id: ComponentId) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }

    pub(crate) const fn new(id: ComponentId) -> Self {
        Self::from_id(id)
    }
}
