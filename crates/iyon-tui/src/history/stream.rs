//! Type-erased semantic stream units for ordered History.

use std::{any::Any, marker::PhantomData};

use crate::{
    View,
    stream::{
        CompiledStream, StreamModel, StreamModelError, StreamOffset, StreamRowIndex,
        StreamSnapshot, StreamingSource, build_index_from, window_view,
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

    pub(crate) fn prepare_from(&self, start: StreamOffset, width: u16) -> StreamRowIndex {
        self.state.prepare_from(start, width)
    }

    pub(crate) fn window_view(&self, index: &StreamRowIndex, top_row: usize, height: u16) -> View {
        self.state.window_view(index, top_row, height)
    }

    pub(crate) fn compile_from(&self, offset: StreamOffset, width: u16) -> CompiledStream {
        self.state.compile_from(offset, width)
    }

    pub(crate) fn release_resident_through(&mut self, offset: StreamOffset) {
        self.state.release_resident_through(offset);
    }

    pub(crate) fn semantic_base(&self) -> StreamOffset {
        self.state.semantic_base()
    }

    pub(crate) fn source_end(&self) -> StreamOffset {
        self.state.source_end()
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
    fn prepare_from(&self, start: StreamOffset, width: u16) -> StreamRowIndex;
    fn window_view(&self, index: &StreamRowIndex, top_row: usize, height: u16) -> View;
    fn compile_from(&self, offset: StreamOffset, width: u16) -> CompiledStream;
    fn release_resident_through(&mut self, offset: StreamOffset);
    fn semantic_base(&self) -> StreamOffset;
    fn source_end(&self) -> StreamOffset;
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

    fn prepare_from(&self, start: StreamOffset, width: u16) -> StreamRowIndex {
        build_index_from(&self.model, start, width)
    }

    fn window_view(&self, index: &StreamRowIndex, top_row: usize, height: u16) -> View {
        window_view(&self.model, index, top_row, height)
    }

    fn compile_from(&self, offset: StreamOffset, width: u16) -> CompiledStream {
        self.model.compile_from(offset, width)
    }

    fn release_resident_through(&mut self, offset: StreamOffset) {
        self.model.release_resident_through(offset);
    }

    fn semantic_base(&self) -> StreamOffset {
        self.model.semantic_base()
    }

    fn source_end(&self) -> StreamOffset {
        self.model.snapshot().source_end()
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
