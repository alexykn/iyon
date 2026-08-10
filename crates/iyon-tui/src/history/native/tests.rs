use std::collections::VecDeque;

use super::super::*;
use crate::{
    Component, ComponentCx, TextSpan,
    backend::NativeHistorySink,
    geometry::Size,
    physical::PhysicalRow,
    presentation::{Insets, IntoView, View, layout::ViewCompiler},
    stream::{
        StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamSnapshotBuilder,
        StreamingSource,
    },
};

#[derive(Clone)]
struct NativeSource {
    snapshot: StreamSnapshot,
    sealed: bool,
}

impl NativeSource {
    fn from_snapshot(snapshot: StreamSnapshot, sealed: bool) -> Self {
        Self { snapshot, sealed }
    }

    fn new(text: &str, stable_through: u64, sealed: bool) -> Self {
        let end = text.len() as u64;
        let snapshot = StreamSnapshotBuilder::new(
            StreamRevision::ZERO,
            StreamOffset::ZERO,
            StreamOffset::new(stable_through),
            StreamOffset::new(end),
        )
        .exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(end)),
            [TextSpan::plain(text)],
        )
        .finish()
        .unwrap();
        Self::from_snapshot(snapshot, sealed)
    }
}

impl StreamingSource for NativeSource {
    fn snapshot(&self) -> StreamSnapshot {
        self.snapshot.clone()
    }

    fn seal(&mut self) {
        self.sealed = true;
        self.snapshot.stable_through = self.snapshot.source_end;
        self.snapshot.revision = StreamRevision::new(self.snapshot.revision.as_u64() + 1);
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }
}

struct LiveComponent;

impl Component for LiveComponent {
    fn view(&self) -> View {
        View::text("B").into_view()
    }

    fn capabilities(&self, _cx: &mut ComponentCx<'_, Self>) {}
}

#[derive(Default)]
struct FakeSink {
    rows: Vec<PhysicalRow>,
    accepts: VecDeque<Result<usize, &'static str>>,
}

impl FakeSink {
    fn accepting(accepted: impl IntoIterator<Item = Result<usize, &'static str>>) -> Self {
        Self {
            accepts: accepted.into_iter().collect(),
            ..Self::default()
        }
    }
}

impl NativeHistorySink for FakeSink {
    type Error = &'static str;

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
        let accepted = self.accepts.pop_front().unwrap_or(Ok(rows.len()))?;
        self.rows.extend(rows[..accepted].iter().cloned());
        Ok(accepted)
    }
}

struct InvalidAckSink;

impl NativeHistorySink for InvalidAckSink {
    type Error = ();

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
        Ok(rows.len() + 1)
    }
}

fn transfer(
    history: &mut History,
    sink: &mut FakeSink,
    width: u16,
    max_rows: usize,
) -> NativeTransferOutcome {
    transfer_native_prefix(history, sink, width, max_rows).unwrap()
}

#[test]
fn invalid_acknowledgement_is_rejected_without_mutation() {
    let mut history = History::new();
    history.push("A").unwrap();
    let before = history.native.clone();
    let result = transfer_native_prefix(&mut history, &mut InvalidAckSink, 8, 8);

    assert_eq!(
        result,
        Err(NativeTransferError::InvalidAcknowledgement {
            requested: 1,
            accepted: 2,
        })
    );
    assert_eq!(history.native, before);
    assert_eq!(history.units.len(), 1);
}

#[test]
fn zero_budget_and_width_do_not_prepare_or_mutate() {
    let mut history = History::new();
    history.push("A").unwrap();
    let before = history.native.clone();
    let mut sink = FakeSink::default();

    assert_eq!(transfer(&mut history, &mut sink, 8, 0).inserted, 0);
    assert_eq!(transfer(&mut history, &mut sink, 0, 8).inserted, 0);
    assert_eq!(history.native, before);
    assert!(sink.rows.is_empty());
}

#[test]
fn static_full_ack_pops_front_and_records_predecessor() {
    let mut history = History::new();
    let id = history.push("A").unwrap();
    let mut sink = FakeSink::default();

    let outcome = transfer(&mut history, &mut sink, 8, 8);

    assert_eq!(outcome.inserted, 1);
    assert!(history.units.is_empty());
    assert_eq!(history.native.last_native_unit, Some(id));
    assert_eq!(sink.rows[0].plain_text(), "A");
    assert_eq!(sink.rows[0].width(), 8);
}

#[test]
fn static_partial_ack_freezes_exact_remainder() {
    let mut history = History::new();
    let id = history.push("A\nB\nC").unwrap();
    let mut sink = FakeSink::accepting([Ok(1)]);

    let outcome = transfer(&mut history, &mut sink, 8, 3);

    assert_eq!(outcome.inserted, 1);
    assert_eq!(history.native.frozen_static.as_ref().unwrap().unit, id);
    assert_eq!(
        history
            .native
            .frozen_static
            .as_ref()
            .unwrap()
            .rows
            .as_slice()[0]
            .plain_text(),
        "B"
    );
    assert_eq!(history.units.front().unwrap().id, id);
}

#[test]
fn sink_zero_and_error_do_not_publish_static_freeze() {
    for response in [Ok(0), Err("blocked")] {
        let mut history = History::new();
        history.push("A\nB").unwrap();
        let before = history.native.clone();
        let mut sink = FakeSink::accepting([response]);

        let result = transfer_native_prefix(&mut history, &mut sink, 8, 2);
        assert!(result.is_err() || result.unwrap().inserted == 0);
        assert_eq!(history.native, before);
        assert_eq!(history.units.len(), 1);
    }
}

#[test]
fn top_padding_is_transferred_once_and_bottom_padding_is_resident() {
    let mut history = History::new();
    history.set_layout(HistoryLayout::new(Insets::vertical(1), 0));
    history.push("A").unwrap();
    let mut sink = FakeSink::default();

    let first = transfer(&mut history, &mut sink, 8, 8);
    assert_eq!(first.inserted, 1);
    assert_eq!(sink.rows.len(), 1);
    assert_eq!(sink.rows[0].width(), 8);
    assert_eq!(history.units.len(), 1);

    let second = transfer(&mut history, &mut sink, 8, 8);
    assert_eq!(second.inserted, 1);
    assert!(history.units.is_empty());
    assert_eq!(sink.rows.len(), 2);

    history.push("B").unwrap();
    assert_eq!(transfer(&mut history, &mut sink, 8, 8).inserted, 1);
    assert_eq!(sink.rows.len(), 3);
}

#[test]
fn live_blocks_later_static_units() {
    let mut registry = crate::component::ComponentRegistry::new();
    let live = registry.register(LiveComponent);
    let mut history = History::new();
    history.push("A").unwrap();
    history.push(View::component(live)).unwrap();
    history.push("C").unwrap();
    let mut sink = FakeSink::default();

    assert_eq!(transfer(&mut history, &mut sink, 8, 8).inserted, 1);
    let outcome = transfer(&mut history, &mut sink, 8, 8);
    assert!(matches!(
        outcome.status,
        NativeTransferStatus::SemanticBlocked { .. }
    ));
    assert_eq!(history.units.len(), 2);
    let _ = registry;
}

#[test]
fn leading_gap_partial_ack_freezes_spacing_across_layout_change() {
    let mut history = History::new();
    history.set_layout(HistoryLayout::new(Insets::ZERO, 3));
    history.push("A").unwrap();
    history.push("B").unwrap();
    let mut sink = FakeSink::default();

    transfer(&mut history, &mut sink, 8, 8);
    let mut partial = FakeSink::accepting([Ok(1)]);
    assert_eq!(transfer(&mut history, &mut partial, 8, 8).inserted, 1);
    assert!(matches!(
        history.native.leading_gap,
        Some(crate::history::native::frontier::SpacingTransferState::Frozen(_))
    ));
    history.set_layout(HistoryLayout::new(Insets::ZERO, 8));
    let Some(crate::history::native::frontier::SpacingTransferState::Frozen(rows)) =
        history.native.leading_gap.as_ref()
    else {
        panic!("expected frozen gap")
    };
    assert_eq!(rows.as_slice().len(), 2);
}

#[test]
fn stream_checkpoint_ack_advances_only_after_sink_ack() {
    let mut history = History::new();
    let handle = history
        .push_stream(NativeSource::new("A\nB", 3, true))
        .unwrap();
    let mut sink = FakeSink::accepting([Ok(1)]);

    let outcome = transfer(&mut history, &mut sink, 8, 1);
    assert_eq!(outcome.inserted, 1);
    let state = history.native.stream.as_ref().unwrap();
    assert!(state.committed_through > StreamOffset::ZERO);
    assert_eq!(state.unit, handle.unit());
    assert_eq!(history.units.front().unwrap().id, handle.unit());
    let registry = crate::component::ComponentRegistry::new();
    let rows = ViewCompiler::default()
        .compile_bounded(
            &project(&history, &registry, Size::new(8, 1)).unwrap().view,
            Size::new(8, 1),
        )
        .rows;
    assert_eq!(rows[0].plain_text(), "B");
}

#[test]
fn atomic_partial_ack_freezes_and_drains_exact_physical_rows() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(4),
        StreamOffset::new(4),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        View::vertical(|column| {
            column.children(["A0", "A1", "A2"]);
        }),
    )
    .unwrap()
    .exact_text(
        StreamRange::new(StreamOffset::new(3), StreamOffset::new(4)),
        [TextSpan::plain("Z")],
    )
    .finish()
    .unwrap();
    let mut history = History::new();
    history
        .push_stream(NativeSource::from_snapshot(snapshot, true))
        .unwrap();
    let mut first = FakeSink::accepting([Ok(1)]);
    assert_eq!(transfer(&mut history, &mut first, 8, 1).inserted, 1);
    assert!(matches!(
        history.native.stream.as_ref().unwrap().partial,
        Some(crate::stream::StreamPartialCommit::FrozenAtomic {
            committed_rows: 1,
            ..
        })
    ));

    let mut second = FakeSink::default();
    assert_eq!(transfer(&mut history, &mut second, 8, 8).inserted, 2);
    assert!(history.native.stream.as_ref().unwrap().partial.is_none());
    assert!(history.native.stream.as_ref().unwrap().committed_through >= StreamOffset::new(3));
}

#[test]
fn stable_zero_height_atomic_advances_without_a_fake_row() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(2),
        StreamOffset::new(2),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
        View::spacer(0),
    )
    .unwrap()
    .exact_text(
        StreamRange::new(StreamOffset::new(1), StreamOffset::new(2)),
        [TextSpan::plain("Z")],
    )
    .finish()
    .unwrap();
    let mut history = History::new();
    history
        .push_stream(NativeSource::from_snapshot(snapshot, true))
        .unwrap();
    let mut sink = FakeSink::default();

    let outcome = transfer(&mut history, &mut sink, 8, 8);
    assert_eq!(outcome.inserted, 1);
    assert_eq!(sink.rows[0].plain_text(), "Z");
    assert!(history.units.is_empty());
}

#[test]
fn unstable_zero_height_atomic_blocks_later_rows() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(2),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
        View::spacer(0),
    )
    .unwrap()
    .exact_text(
        StreamRange::new(StreamOffset::new(1), StreamOffset::new(2)),
        [TextSpan::plain("Z")],
    )
    .finish()
    .unwrap();
    let mut history = History::new();
    history
        .push_stream(NativeSource::from_snapshot(snapshot, false))
        .unwrap();
    let mut sink = FakeSink::default();

    let outcome = transfer(&mut history, &mut sink, 8, 8);
    assert!(matches!(
        outcome.status,
        NativeTransferStatus::SemanticBlocked { .. }
    ));
    assert!(sink.rows.is_empty());
    assert_eq!(history.units.len(), 1);
}

#[test]
fn sealed_empty_stream_retires_without_a_sink_write() {
    let mut history = History::new();
    history.push_stream(NativeSource::new("", 0, true)).unwrap();
    let mut sink = FakeSink::default();

    let outcome = transfer(&mut history, &mut sink, 8, 8);
    assert_eq!(outcome.inserted, 0);
    assert!(sink.rows.is_empty());
    assert!(history.units.is_empty());
}

#[test]
fn open_empty_stream_remains_resident() {
    let mut history = History::new();
    history
        .push_stream(NativeSource::new("", 0, false))
        .unwrap();
    let mut sink = FakeSink::default();

    transfer(&mut history, &mut sink, 8, 8);
    assert_eq!(history.units.len(), 1);
}

#[test]
fn native_predecessor_keeps_default_gap_in_resident_projection() {
    let mut history = History::new();
    history.set_layout(HistoryLayout::new(Insets::ZERO, 1));
    history.push("A").unwrap();
    history.push("B").unwrap();
    let registry = crate::component::ComponentRegistry::new();
    let mut sink = FakeSink::default();

    transfer(&mut history, &mut sink, 8, 8);
    let rows = ViewCompiler::default()
        .compile_bounded(
            &project(&history, &registry, Size::new(8, 2)).unwrap().view,
            Size::new(8, 2),
        )
        .rows;
    assert_eq!(
        rows.iter().map(PhysicalRow::plain_text).collect::<Vec<_>>(),
        ["", "B"]
    );

    transfer(&mut history, &mut sink, 8, 8);
    let rows = ViewCompiler::default()
        .compile_bounded(
            &project(&history, &registry, Size::new(8, 2)).unwrap().view,
            Size::new(8, 2),
        )
        .rows;
    assert_eq!(
        rows.iter().map(PhysicalRow::plain_text).collect::<Vec<_>>(),
        ["", "B"]
    );
}

#[test]
fn attach_to_previous_does_not_create_frontier_gap() {
    let mut history = History::new();
    history.push("A").unwrap();
    history
        .push_with_boundary(FlowBoundary::AttachToPrevious, "B")
        .unwrap();
    let registry = crate::component::ComponentRegistry::new();
    let mut sink = FakeSink::default();
    transfer(&mut history, &mut sink, 8, 8);

    let rows = ViewCompiler::default()
        .compile_bounded(
            &project(&history, &registry, Size::new(8, 1)).unwrap().view,
            Size::new(8, 1),
        )
        .rows;
    assert_eq!(
        rows.iter().map(PhysicalRow::plain_text).collect::<Vec<_>>(),
        ["B"]
    );
}

#[test]
fn frozen_static_remainder_is_exposed_as_a_physical_overlay() {
    let mut history = History::new();
    history.push("A\nB\nC").unwrap();
    let registry = crate::component::ComponentRegistry::new();
    let mut sink = FakeSink::accepting([Ok(1)]);
    transfer(&mut history, &mut sink, 8, 3);

    let (_, overlay) = project_with_overlay(&history, &registry, Size::new(8, 2)).unwrap();
    let overlay = overlay.expect("frozen remainder should be overlaid");
    assert_eq!(
        overlay
            .rows
            .iter()
            .map(PhysicalRow::plain_text)
            .collect::<Vec<_>>(),
        ["B", "C"]
    );
}

#[test]
fn static_physical_rows_match_bounded_view_rows() {
    let mut history = History::new();
    history.set_layout(HistoryLayout::new(Insets::horizontal(2), 0));
    history.push("wide").unwrap();
    let mut sink = FakeSink::default();
    transfer(&mut history, &mut sink, 10, 4);

    let expected = ViewCompiler::default()
        .compile_bounded(&View::text("wide").into_view(), Size::new(6, 1))
        .rows[0]
        .clone();
    assert_eq!(sink.rows[0].width(), 10);
    assert_eq!(sink.rows[0].cell(2), expected.cell(0));
}
