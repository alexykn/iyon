//! Ordered semantic History model.

use std::{collections::VecDeque, time::Instant};

use crate::{presentation::IntoView, stream::StreamingSource};

use super::{
    ErasedHistoryStream, FlowBoundary, HistoryError, HistoryLayout, HistoryStreamHandle,
    HistoryUnit, HistoryUnitContent, HistoryUnitId,
};

/// An ordered root-level historical, live, or streaming semantic flow.
///
/// History owns unit order, semantic lifetime, and semantic layout. Native
/// durability remains private behind the host-owned native sink seam.
///
/// ```no_run
/// use iyon_tui::{Component, ComponentHandle, History, HistoryError, View};
///
/// fn build<C: Component>(handle: ComponentHandle<C>) -> Result<History, HistoryError> {
///     let mut history = History::new();
///     history.push("completed output")?;
///     let live = history.push(View::component(handle))?;
///     history.freeze(live, "final output")?;
///     Ok(history)
/// }
/// ```
pub struct History {
    pub(super) units: VecDeque<HistoryUnit>,
    layout: HistoryLayout,
    pub(super) native: super::native::NativeFrontier,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            units: VecDeque::new(),
            layout: HistoryLayout::default(),
            native: super::native::NativeFrontier::default(),
        }
    }

    pub fn push(&mut self, view: impl IntoView) -> Result<HistoryUnitId, HistoryError> {
        self.push_with_boundary(view, FlowBoundary::Default)
    }

    pub fn push_with_boundary(
        &mut self,
        view: impl IntoView,
        boundary: FlowBoundary,
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

    /// Replaces a tail Live unit with a typed open Stream without changing its
    /// identity or flow boundary.
    pub fn replace_live_with_stream<S: StreamingSource>(
        &mut self,
        unit: HistoryUnitId,
        source: S,
    ) -> Result<HistoryStreamHandle<S>, HistoryError> {
        let index = self.index_of(unit)?;
        if index + 1 != self.units.len() {
            return Err(HistoryError::LiveMustRemainTail { unit });
        }
        if !matches!(self.units[index].content, HistoryUnitContent::Live(_)) {
            return Err(HistoryError::UnitNotLive { unit });
        }
        let stream = ErasedHistoryStream::new(source).map_err(HistoryError::Stream)?;
        self.units[index].content = HistoryUnitContent::Stream(stream);
        Ok(HistoryStreamHandle::new(unit))
    }

    /// Discards a transient tail Live unit without creating spacing or native
    /// history rows.
    pub fn discard_live(&mut self, unit: HistoryUnitId) -> Result<(), HistoryError> {
        let index = self.index_of(unit)?;
        if index + 1 != self.units.len() {
            return Err(HistoryError::LiveMustRemainTail { unit });
        }
        if !matches!(self.units[index].content, HistoryUnitContent::Live(_)) {
            return Err(HistoryError::UnitNotLive { unit });
        }
        self.units.remove(index);
        Ok(())
    }

    pub fn freeze(
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

    /// Attaches one typed semantic Stream as the History tail.
    ///
    /// ```text
    /// let stream = history.push_stream(source)?;
    /// history.update_stream(stream, |source| { /* mutate source */ })?;
    /// history.seal_stream(stream)?;
    /// history.push("next unit")?;
    /// ```
    pub fn push_stream<S: StreamingSource>(
        &mut self,
        source: S,
    ) -> Result<HistoryStreamHandle<S>, HistoryError> {
        self.push_stream_with_boundary(source, FlowBoundary::Default)
    }

    pub fn push_stream_with_boundary<S: StreamingSource>(
        &mut self,
        source: S,
        boundary: FlowBoundary,
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

    pub fn update_stream<S: StreamingSource, R>(
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

    #[allow(dead_code)]
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

    pub(crate) fn next_stream_wakeup(&self) -> Option<Instant> {
        self.units
            .iter()
            .filter_map(|unit| match &unit.content {
                HistoryUnitContent::Stream(stream) => stream.next_wakeup(),
                _ => None,
            })
            .min()
    }

    pub(crate) fn advance_streams(&mut self, now: Instant) -> Result<bool, HistoryError> {
        let mut changed = false;
        for unit in &mut self.units {
            if let HistoryUnitContent::Stream(stream) = &mut unit.content {
                changed |= stream.advance(now)?;
            }
        }
        Ok(changed)
    }

    pub fn seal_stream<S: StreamingSource>(
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

    pub fn layout(&self) -> HistoryLayout {
        self.layout
    }

    pub fn set_layout(&mut self, layout: HistoryLayout) {
        self.layout = layout;
    }

    pub fn with_layout(mut self, layout: HistoryLayout) -> Self {
        self.layout = layout;
        self
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
