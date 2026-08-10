//! Private monotonic native-history ownership for generic History.

pub(super) mod frontier;

#[cfg(test)]
mod tests;

use crate::{
    backend::NativeHistorySink,
    physical::PhysicalRow,
    presentation::layout::compile_view,
    stream::{
        CompiledStream, FrozenPhysicalRows, StreamPartialCommit, StreamTransferPayload,
        plan_stream_transfer,
    },
};

use super::{FlowBoundary, History, HistoryUnitContent, HistoryUnitId};
pub(super) use frontier::NativeFrontier;
use frontier::{FrozenStaticRemainder, SpacingTransferState, StreamFrontierState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeTransferStatus {
    Idle,
    Progress,
    SinkBlocked,
    SemanticBlocked { unit: HistoryUnitId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTransferOutcome {
    pub(crate) requested: usize,
    pub(crate) inserted: usize,
    pub(crate) status: NativeTransferStatus,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NativeTransferError<E> {
    Sink(E),
    InvalidAcknowledgement { requested: usize, accepted: usize },
}

pub(crate) fn transfer_native_prefix<S: NativeHistorySink>(
    history: &mut History,
    sink: &mut S,
    width: u16,
    max_rows: usize,
) -> Result<NativeTransferOutcome, NativeTransferError<S::Error>> {
    if max_rows == 0 || width == 0 || history.units.is_empty() {
        return Ok(outcome(0, 0, NativeTransferStatus::Idle));
    }

    if history.units.is_empty() {
        return Ok(outcome(0, 0, NativeTransferStatus::Progress));
    }

    if let Some(rows) = prepare_top_padding(history, width) {
        return transfer_spacing(&mut history.native.top_padding, sink, rows, max_rows);
    }

    let gap_was_uninitialized = history.native.leading_gap.is_none();
    if let Some(rows) = prepare_leading_gap(history, width) {
        let result = transfer_spacing(
            history.native.leading_gap.as_mut().expect("gap state"),
            sink,
            rows,
            max_rows,
        );
        if gap_was_uninitialized
            && result
                .as_ref()
                .map_or(true, |outcome| outcome.inserted == 0)
        {
            history.native.leading_gap = None;
        }
        return result;
    }

    if history.native.frozen_static.is_some() {
        return transfer_frozen_static(history, sink, max_rows);
    }

    let unit_id = history.units.front().expect("nonempty History").id;
    match &history.units.front().expect("nonempty History").content {
        HistoryUnitContent::Live(_) => Ok(outcome(
            0,
            0,
            NativeTransferStatus::SemanticBlocked { unit: unit_id },
        )),
        HistoryUnitContent::Static(view) => {
            let rows = static_rows(view, width, history.layout());
            if rows.is_empty() {
                retire_front(history);
                return transfer_native_prefix(history, sink, width, max_rows);
            }
            transfer_static(history, sink, rows, max_rows)
        }
        HistoryUnitContent::Stream(_) => transfer_stream(history, sink, width, max_rows),
    }
}

fn outcome(
    requested: usize,
    inserted: usize,
    status: NativeTransferStatus,
) -> NativeTransferOutcome {
    NativeTransferOutcome {
        requested,
        inserted,
        status,
    }
}

fn prepare_top_padding(history: &mut History, width: u16) -> Option<Vec<PhysicalRow>> {
    match &history.native.top_padding {
        SpacingTransferState::Native => None,
        SpacingTransferState::Frozen(rows) => Some(rows.as_slice().to_vec()),
        SpacingTransferState::Semantic => {
            let count = usize::from(history.layout().padding.top);
            if count == 0 {
                None
            } else {
                Some(NativeFrontier::blank_rows(width, count))
            }
        }
    }
}

fn prepare_leading_gap(history: &mut History, width: u16) -> Option<Vec<PhysicalRow>> {
    if history.native.last_native_unit.is_none() {
        return None;
    }
    let unit = history.units.front().expect("nonempty History");
    if !matches!(unit.boundary, FlowBoundary::Default) {
        return None;
    }
    if history.native.leading_gap.is_none() {
        if history.layout().gap == 0 {
            return None;
        }
        history.native.leading_gap = Some(SpacingTransferState::Semantic);
    }
    let state = history.native.leading_gap.as_ref().expect("gap state");
    match state {
        SpacingTransferState::Native => None,
        SpacingTransferState::Frozen(rows) => Some(rows.as_slice().to_vec()),
        SpacingTransferState::Semantic => {
            let count = usize::from(history.layout().gap);
            if count == 0 {
                history.native.leading_gap = Some(SpacingTransferState::Native);
                None
            } else {
                Some(NativeFrontier::blank_rows(width, count))
            }
        }
    }
}

fn transfer_spacing<S: NativeHistorySink>(
    state: &mut SpacingTransferState,
    sink: &mut S,
    rows: Vec<PhysicalRow>,
    max_rows: usize,
) -> Result<NativeTransferOutcome, NativeTransferError<S::Error>> {
    let request = rows.len().min(max_rows);
    let accepted = sink
        .insert_history_rows(&rows[..request])
        .map_err(NativeTransferError::Sink)?;
    if accepted > request {
        return Err(NativeTransferError::InvalidAcknowledgement {
            requested: request,
            accepted,
        });
    }
    if accepted == 0 {
        return Ok(outcome(request, 0, NativeTransferStatus::SinkBlocked));
    }
    if accepted == rows.len() {
        *state = SpacingTransferState::Native;
    } else {
        *state = SpacingTransferState::Frozen(FrozenPhysicalRows::new(rows[accepted..].to_vec()));
    }
    Ok(outcome(request, accepted, NativeTransferStatus::Progress))
}

fn transfer_static<S: NativeHistorySink>(
    history: &mut History,
    sink: &mut S,
    rows: Vec<PhysicalRow>,
    max_rows: usize,
) -> Result<NativeTransferOutcome, NativeTransferError<S::Error>> {
    let request = rows.len().min(max_rows);
    let accepted = sink
        .insert_history_rows(&rows[..request])
        .map_err(NativeTransferError::Sink)?;
    if accepted > request {
        return Err(NativeTransferError::InvalidAcknowledgement {
            requested: request,
            accepted,
        });
    }
    if accepted == 0 {
        return Ok(outcome(request, 0, NativeTransferStatus::SinkBlocked));
    }
    if accepted == rows.len() {
        retire_front(history);
    } else {
        history.native.frozen_static = Some(FrozenStaticRemainder {
            unit: history.units.front().expect("static unit").id,
            rows: FrozenPhysicalRows::new(rows[accepted..].to_vec()),
        });
    }
    Ok(outcome(request, accepted, NativeTransferStatus::Progress))
}

fn transfer_frozen_static<S: NativeHistorySink>(
    history: &mut History,
    sink: &mut S,
    max_rows: usize,
) -> Result<NativeTransferOutcome, NativeTransferError<S::Error>> {
    let frozen = history
        .native
        .frozen_static
        .as_ref()
        .expect("frozen static");
    let rows = frozen.rows.as_slice();
    let request = rows.len().min(max_rows);
    let accepted = sink
        .insert_history_rows(&rows[..request])
        .map_err(NativeTransferError::Sink)?;
    if accepted > request {
        return Err(NativeTransferError::InvalidAcknowledgement {
            requested: request,
            accepted,
        });
    }
    if accepted == 0 {
        return Ok(outcome(request, 0, NativeTransferStatus::SinkBlocked));
    }
    if accepted == rows.len() {
        retire_front(history);
    } else {
        let remainder = rows[accepted..].to_vec();
        history.native.frozen_static.as_mut().unwrap().rows = FrozenPhysicalRows::new(remainder);
    }
    Ok(outcome(request, accepted, NativeTransferStatus::Progress))
}

fn transfer_stream<S: NativeHistorySink>(
    history: &mut History,
    sink: &mut S,
    width: u16,
    max_rows: usize,
) -> Result<NativeTransferOutcome, NativeTransferError<S::Error>> {
    let unit = history.units.front().expect("stream unit");
    let unit_id = unit.id;
    let (starting_cursor, starting_partial) = history
        .native
        .stream
        .as_ref()
        .map_or((crate::stream::StreamOffset::ZERO, None), |state| {
            (state.committed_through, state.partial.clone())
        });
    let start = match starting_partial.as_ref() {
        Some(StreamPartialCommit::FrozenAtomic { source_end, .. }) => *source_end,
        None => starting_cursor,
    };
    let (mut compiled, sealed, source_end) = {
        let stream = match &unit.content {
            HistoryUnitContent::Stream(stream) => stream,
            _ => unreachable!("stream frontier must match stream unit"),
        };
        (
            stream.compile_from(start, width),
            stream.is_sealed(),
            stream.source_end(),
        )
    };
    place_stream_rows(&mut compiled, width, history.layout());
    if starting_partial.is_none()
        && compiled
            .zero_row_prefix
            .is_some_and(|offset| offset > starting_cursor)
    {
        let next = compiled.zero_row_prefix.expect("checked zero-row prefix");
        let HistoryUnitContent::Stream(stream) = &mut history.units.front_mut().unwrap().content
        else {
            unreachable!("stream frontier must match stream unit")
        };
        stream.release_resident_through(next);
        let state = history.native.stream.get_or_insert(StreamFrontierState {
            unit: unit_id,
            committed_through: starting_cursor,
            partial: None,
        });
        state.committed_through = next;
        return transfer_native_prefix(history, sink, width, max_rows);
    }
    let plan = plan_stream_transfer(
        &compiled,
        max_rows,
        starting_partial.as_ref(),
        starting_cursor,
    );
    let rows = payload_rows(&compiled, &plan.payload);
    if rows.is_empty() {
        if sealed && starting_partial.is_none() && starting_cursor >= source_end {
            retire_front(history);
            return Ok(outcome(0, 0, NativeTransferStatus::Progress));
        }
        return Ok(outcome(
            0,
            0,
            NativeTransferStatus::SemanticBlocked { unit: unit_id },
        ));
    }

    let request = rows.len();
    let accepted = sink
        .insert_history_rows(&rows)
        .map_err(NativeTransferError::Sink)?;
    if accepted > request {
        return Err(NativeTransferError::InvalidAcknowledgement {
            requested: request,
            accepted,
        });
    }
    if accepted == 0 {
        return Ok(outcome(request, 0, NativeTransferStatus::SinkBlocked));
    }

    let accepted_plan = plan_stream_transfer(
        &compiled,
        accepted,
        starting_partial.as_ref(),
        starting_cursor,
    );
    let new_cursor = accepted_plan.next_committed_through;
    let new_partial = accepted_plan.next_partial;
    if new_cursor > starting_cursor {
        let HistoryUnitContent::Stream(stream) = &mut history.units.front_mut().unwrap().content
        else {
            unreachable!("stream frontier must match stream unit")
        };
        stream.release_resident_through(new_cursor);
    }
    let state = history.native.stream.get_or_insert(StreamFrontierState {
        unit: unit_id,
        committed_through: starting_cursor,
        partial: starting_partial,
    });
    state.committed_through = new_cursor;
    state.partial = new_partial;
    if sealed && state.partial.is_none() && new_cursor >= source_end {
        retire_front(history);
    }
    Ok(outcome(request, accepted, NativeTransferStatus::Progress))
}

fn payload_rows(compiled: &CompiledStream, payload: &StreamTransferPayload) -> Vec<PhysicalRow> {
    match payload {
        StreamTransferPayload::Compiled { start, len } => compiled.rows
            [*start..start.saturating_add(*len)]
            .iter()
            .map(|row| row.physical.clone())
            .collect(),
        StreamTransferPayload::Frozen { rows } => rows.as_slice().to_vec(),
    }
}

fn place_stream_rows(compiled: &mut CompiledStream, width: u16, layout: super::HistoryLayout) {
    let content_width =
        width.saturating_sub(layout.padding.left.saturating_add(layout.padding.right));
    for row in &mut compiled.rows {
        row.physical = row
            .physical
            .placed(width, layout.padding.left.min(content_width));
    }
}

fn static_rows(
    view: &crate::presentation::View,
    width: u16,
    layout: super::HistoryLayout,
) -> Vec<PhysicalRow> {
    let content_width =
        width.saturating_sub(layout.padding.left.saturating_add(layout.padding.right));
    compile_view(view, content_width)
        .rows
        .into_iter()
        .map(|row| row.placed(width, layout.padding.left))
        .collect()
}

fn retire_front(history: &mut History) {
    let unit = history
        .units
        .pop_front()
        .expect("retiring nonempty History");
    history.native.last_native_unit = Some(unit.id);
    history.native.reset_unit_state();
}
