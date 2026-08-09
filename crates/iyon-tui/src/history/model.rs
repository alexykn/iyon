//! Private ordered semantic History model.

use std::collections::VecDeque;

use crate::{presentation::IntoView, stream::StreamingSource};

use super::{
    ErasedHistoryStream, FlowBoundary, HistoryError, HistoryStreamHandle, HistoryUnit,
    HistoryUnitContent, HistoryUnitId,
};

/// Optional ordered semantic history capability.
///
/// History owns unit order and semantic lifetimes only. It has no viewport,
/// layout, physical rows, native history, or terminal-writing behavior.
pub(crate) struct History {
    units: VecDeque<HistoryUnit>,
}

impl History {
    pub(crate) fn new() -> Self {
        Self {
            units: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, view: impl IntoView) -> Result<HistoryUnitId, HistoryError> {
        self.push_with_boundary(FlowBoundary::Default, view)
    }

    pub(crate) fn push_with_boundary(
        &mut self,
        boundary: FlowBoundary,
        view: impl IntoView,
    ) -> Result<HistoryUnitId, HistoryError> {
        self.ensure_append_allowed()?;
        let view = view.into_view();
        let content = if view.contains_component_identity() {
            HistoryUnitContent::Live(view)
        } else {
            HistoryUnitContent::Static(view)
        };
        let id = HistoryUnitId::allocate();
        self.units.push_back(HistoryUnit {
            id,
            boundary,
            content,
        });
        Ok(id)
    }

    pub(crate) fn freeze(
        &mut self,
        unit: HistoryUnitId,
        final_view: impl IntoView,
    ) -> Result<(), HistoryError> {
        let index = self.index_of(unit)?;
        if !matches!(self.units[index].content, HistoryUnitContent::Live(_)) {
            return Err(HistoryError::UnitNotLive { unit });
        }
        let final_view = final_view.into_view();
        if final_view.contains_component_identity() {
            return Err(HistoryError::FinalViewContainsComponent { unit });
        }
        self.units[index].content = HistoryUnitContent::Static(final_view);
        Ok(())
    }

    pub(crate) fn push_stream<S: StreamingSource>(
        &mut self,
        source: S,
    ) -> Result<HistoryStreamHandle<S>, HistoryError> {
        self.push_stream_with_boundary(FlowBoundary::Default, source)
    }

    pub(crate) fn push_stream_with_boundary<S: StreamingSource>(
        &mut self,
        boundary: FlowBoundary,
        source: S,
    ) -> Result<HistoryStreamHandle<S>, HistoryError> {
        self.ensure_append_allowed()?;
        let stream = ErasedHistoryStream::new(source).map_err(HistoryError::Stream)?;
        let id = HistoryUnitId::allocate();
        self.units.push_back(HistoryUnit {
            id,
            boundary,
            content: HistoryUnitContent::Stream(stream),
        });
        Ok(HistoryStreamHandle::new(id))
    }

    pub(crate) fn update_stream<S: StreamingSource, R>(
        &mut self,
        handle: HistoryStreamHandle<S>,
        update: impl FnOnce(&mut S) -> R,
    ) -> Result<R, HistoryError> {
        let index = self.index_of(handle.unit())?;
        let history_unit = &mut self.units[index];
        let HistoryUnitContent::Stream(stream) = &mut history_unit.content else {
            return Err(HistoryError::UnitNotStream {
                unit: handle.unit(),
            });
        };
        stream.update(handle.unit(), update)
    }

    pub(crate) fn refresh_stream<S: StreamingSource>(
        &mut self,
        handle: HistoryStreamHandle<S>,
    ) -> Result<(), HistoryError> {
        let index = self.index_of(handle.unit())?;
        let history_unit = &mut self.units[index];
        let HistoryUnitContent::Stream(stream) = &mut history_unit.content else {
            return Err(HistoryError::UnitNotStream {
                unit: handle.unit(),
            });
        };
        stream.refresh::<S>(handle.unit())
    }

    pub(crate) fn seal_stream<S: StreamingSource>(
        &mut self,
        handle: HistoryStreamHandle<S>,
    ) -> Result<(), HistoryError> {
        let index = self.index_of(handle.unit())?;
        let history_unit = &mut self.units[index];
        let HistoryUnitContent::Stream(stream) = &mut history_unit.content else {
            return Err(HistoryError::UnitNotStream {
                unit: handle.unit(),
            });
        };
        stream.seal::<S>(handle.unit())
    }

    pub(super) fn units(&self) -> impl Iterator<Item = &HistoryUnit> {
        self.units.iter()
    }

    fn ensure_append_allowed(&self) -> Result<(), HistoryError> {
        let Some(last) = self.units.back() else {
            return Ok(());
        };
        if let HistoryUnitContent::Stream(stream) = &last.content {
            if !stream.is_sealed() {
                return Err(HistoryError::OpenStreamMustRemainTail { stream: last.id });
            }
        }
        Ok(())
    }

    fn index_of(&self, id: HistoryUnitId) -> Result<usize, HistoryError> {
        self.units
            .iter()
            .position(|unit| unit.id == id)
            .ok_or(HistoryError::UnitNotFound { unit: id })
    }
}
