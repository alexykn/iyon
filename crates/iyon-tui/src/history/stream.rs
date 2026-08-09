//! Type-erased semantic stream units for ordered History.

use std::{any::Any, marker::PhantomData};

use crate::stream::{StreamModel, StreamModelError, StreamSnapshot, StreamingSource};

use super::{HistoryError, HistoryUnitId};

pub(crate) struct ErasedHistoryStream {
    state: Box<dyn ErasedHistoryStreamState>,
}

impl ErasedHistoryStream {
    pub(crate) fn new<S: StreamingSource>(source: S) -> Result<Self, StreamModelError> {
        let typed = TypedHistoryStream::new(source)?;
        Ok(Self {
            state: Box::new(typed),
        })
    }

    pub(crate) fn is_sealed(&self) -> bool {
        self.state.is_sealed()
    }

    pub(crate) fn snapshot(&self) -> &StreamSnapshot {
        self.state.snapshot()
    }

    pub(crate) fn refresh(&mut self, unit: HistoryUnitId) -> Result<(), HistoryError> {
        self.state.refresh(unit)
    }

    pub(crate) fn seal(&mut self, unit: HistoryUnitId) -> Result<(), HistoryError> {
        self.state.seal(unit)
    }

    pub(crate) fn update<S: StreamingSource, R>(
        &mut self,
        unit: HistoryUnitId,
        update: impl FnOnce(&mut S) -> R,
    ) -> Result<R, HistoryError> {
        let Some(typed) = self
            .state
            .as_any_mut()
            .downcast_mut::<TypedHistoryStream<S>>()
        else {
            return Err(HistoryError::StreamTypeMismatch { unit });
        };
        if typed.sealed {
            return Err(HistoryError::StreamAlreadySealed { unit });
        }
        Ok(update(typed.model.source_mut()))
    }
}

trait ErasedHistoryStreamState: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn is_sealed(&self) -> bool;
    fn snapshot(&self) -> &StreamSnapshot;
    fn refresh(&mut self, unit: HistoryUnitId) -> Result<(), HistoryError>;
    fn seal(&mut self, unit: HistoryUnitId) -> Result<(), HistoryError>;
}

struct TypedHistoryStream<S: StreamingSource> {
    model: StreamModel<S>,
    sealed: bool,
    published: Option<StreamSnapshot>,
}

impl<S: StreamingSource> TypedHistoryStream<S> {
    fn new(source: S) -> Result<Self, StreamModelError> {
        let model = StreamModel::new(source)?;
        let sealed = model.source().is_sealed();
        if sealed && model.snapshot().stable_through() != model.snapshot().source_end() {
            return Err(StreamModelError::UnstableAfterSeal);
        }
        let published = sealed.then(|| model.snapshot().clone());
        Ok(Self {
            model,
            sealed,
            published,
        })
    }
}

impl<S: StreamingSource> ErasedHistoryStreamState for TypedHistoryStream<S> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }

    fn snapshot(&self) -> &StreamSnapshot {
        self.model.snapshot()
    }

    fn refresh(&mut self, unit: HistoryUnitId) -> Result<(), HistoryError> {
        if self.sealed {
            let observed = self.model.source().snapshot();
            if !self.model.source().is_sealed() || self.published.as_ref() != Some(&observed) {
                return Err(HistoryError::SealedStreamChanged { unit });
            }
            return Ok(());
        }
        self.model.refresh().map_err(HistoryError::Stream)
    }

    fn seal(&mut self, unit: HistoryUnitId) -> Result<(), HistoryError> {
        if self.sealed {
            let observed = self.model.source().snapshot();
            if !self.model.source().is_sealed() || self.published.as_ref() != Some(&observed) {
                return Err(HistoryError::SealedStreamChanged { unit });
            }
            return Ok(());
        }
        let resident = self.model.resident().clone();
        let snapshot = self.model.snapshot().clone();
        if let Err(error) = self.model.seal() {
            self.model.restore_semantic_state(resident, snapshot);
            return Err(HistoryError::Stream(error));
        }
        self.sealed = true;
        self.published = Some(self.model.snapshot().clone());
        Ok(())
    }
}

/// Typed, non-owning identity for a History stream unit.
pub(crate) struct HistoryStreamHandle<S: StreamingSource> {
    unit: HistoryUnitId,
    marker: PhantomData<fn() -> S>,
}

impl<S: StreamingSource> Copy for HistoryStreamHandle<S> {}

impl<S: StreamingSource> Clone for HistoryStreamHandle<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: StreamingSource> PartialEq for HistoryStreamHandle<S> {
    fn eq(&self, other: &Self) -> bool {
        self.unit == other.unit
    }
}

impl<S: StreamingSource> Eq for HistoryStreamHandle<S> {}

impl<S: StreamingSource> std::fmt::Debug for HistoryStreamHandle<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("HistoryStreamHandle")
    }
}

impl<S: StreamingSource> HistoryStreamHandle<S> {
    pub(crate) const fn new(unit: HistoryUnitId) -> Self {
        Self {
            unit,
            marker: PhantomData,
        }
    }

    pub(crate) const fn unit(self) -> HistoryUnitId {
        self.unit
    }
}
