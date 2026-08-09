//! Type-erased semantic stream units for ordered History.

use std::{any::Any, marker::PhantomData};

use crate::{
    View,
    stream::{
        StreamModel, StreamModelError, StreamRowIndex, StreamSnapshot, StreamingSource,
        build_index, window_view,
    },
};

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

    pub(crate) fn row_count(&self, width: u16) -> usize {
        self.state.row_count(width)
    }

    pub(crate) fn window_view(&self, width: u16, top_row: usize, height: u16) -> View {
        self.state.window_view(width, top_row, height)
    }

    pub(crate) fn refresh<S: StreamingSource>(
        &mut self,
        unit: HistoryUnitId,
    ) -> Result<(), HistoryError> {
        self.typed_mut::<S>(unit)?.refresh(unit)
    }

    pub(crate) fn seal<S: StreamingSource>(
        &mut self,
        unit: HistoryUnitId,
    ) -> Result<(), HistoryError> {
        self.typed_mut::<S>(unit)?.seal(unit)
    }

    pub(crate) fn update<S: StreamingSource, R>(
        &mut self,
        unit: HistoryUnitId,
        update: impl FnOnce(&mut S) -> R,
    ) -> Result<R, HistoryError> {
        let typed = self.typed_mut::<S>(unit)?;
        if typed.sealed {
            return Err(HistoryError::StreamAlreadySealed { unit });
        }
        let result = update(typed.model.source_mut());
        typed.model.refresh().map_err(HistoryError::Stream)?;
        Ok(result)
    }

    fn typed_mut<S: StreamingSource>(
        &mut self,
        unit: HistoryUnitId,
    ) -> Result<&mut TypedHistoryStream<S>, HistoryError> {
        self.state
            .as_any_mut()
            .downcast_mut::<TypedHistoryStream<S>>()
            .ok_or(HistoryError::StreamTypeMismatch { unit })
    }
}

trait ErasedHistoryStreamState: Any {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn is_sealed(&self) -> bool;
    fn snapshot(&self) -> &StreamSnapshot;
    fn row_count(&self, width: u16) -> usize;
    fn window_view(&self, width: u16, top_row: usize, height: u16) -> View;
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

impl<S: StreamingSource> TypedHistoryStream<S> {
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
        self.model.seal().map_err(HistoryError::Stream)?;
        self.sealed = true;
        self.published = Some(self.model.snapshot().clone());
        Ok(())
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

    fn row_count(&self, width: u16) -> usize {
        build_index(&self.model, width).anchors.len()
    }

    fn window_view(&self, width: u16, top_row: usize, height: u16) -> View {
        let index: StreamRowIndex = build_index(&self.model, width);
        window_view(&self.model, &index, top_row, height)
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
