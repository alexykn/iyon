use super::super::*;
use super::TestSource;
use crate::{
    View,
    component::ComponentId,
    presentation::IntoView,
    stream::{StreamModelError, StreamOffset, StreamRevision, StreamSnapshot, StreamingSource},
};

struct OtherSource {
    inner: TestSource,
}

struct FailingSealSource {
    inner: TestSource,
}

impl StreamingSource for FailingSealSource {
    fn snapshot(&self) -> StreamSnapshot {
        self.inner.snapshot()
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        self.inner.compact_before(offset);
    }

    fn seal(&mut self) {
        let mut state = self.inner.state.borrow_mut();
        state.sealed = true;
        state.snapshot.revision = StreamRevision::new(state.snapshot.revision().as_u64() + 1);
    }

    fn is_sealed(&self) -> bool {
        self.inner.is_sealed()
    }
}

impl StreamingSource for OtherSource {
    fn snapshot(&self) -> StreamSnapshot {
        self.inner.snapshot()
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        self.inner.compact_before(offset);
    }

    fn seal(&mut self) {
        self.inner.seal();
    }

    fn is_sealed(&self) -> bool {
        self.inner.is_sealed()
    }
}

fn live_view() -> View {
    View::text("live")
        .into_view()
        .with_component(ComponentId::allocate())
}

fn stream_snapshot(history: &History, id: HistoryUnitId) -> StreamSnapshot {
    let unit = history.units().find(|unit| unit.id == id).unwrap();
    let HistoryUnitContent::Stream(stream) = &unit.content else {
        panic!("expected stream unit");
    };
    stream.snapshot().clone()
}

#[test]
fn open_stream_must_remain_logical_tail() {
    let mut history = History::new();
    let first = history.push("first").unwrap();
    let stream = history.push_stream(TestSource::new("A", 0, false)).unwrap();
    let before = unit_ids(&history);

    assert_eq!(
        history.push("blocked"),
        Err(HistoryError::OpenStreamMustRemainTail {
            stream: stream.unit(),
        })
    );
    assert_eq!(
        history.push(live_view()),
        Err(HistoryError::OpenStreamMustRemainTail {
            stream: stream.unit(),
        })
    );
    assert!(matches!(
        history.push_stream(TestSource::new("B", 0, false)),
        Err(HistoryError::OpenStreamMustRemainTail { stream: id }) if id == stream.unit()
    ));
    assert_eq!(unit_ids(&history), before);
    assert_eq!(unit_ids(&history), vec![first, stream.unit()]);
}

#[test]
fn sealing_preserves_stream_identity_and_unlocks_append() {
    let mut history = History::new();
    let stream = history
        .push_stream_with_boundary(
            FlowBoundary::AttachToPrevious,
            TestSource::new("A", 0, false),
        )
        .unwrap();
    history.seal_stream(stream).unwrap();
    history.seal_stream(stream).unwrap();

    let unit = history.units().next().unwrap();
    assert_eq!(unit.id, stream.unit());
    assert_eq!(unit.boundary, FlowBoundary::AttachToPrevious);
    let HistoryUnitContent::Stream(stream_content) = &unit.content else {
        panic!("expected stream unit");
    };
    assert!(stream_content.is_sealed());

    let static_id = history.push("after stream").unwrap();
    let live_id = history.push(live_view()).unwrap();
    assert_eq!(unit_ids(&history), vec![stream.unit(), static_id, live_id]);
    assert!(matches!(
        &history.units().next().unwrap().content,
        HistoryUnitContent::Stream(stream) if stream.is_sealed()
    ));
}

#[test]
fn earlier_live_unit_can_freeze_while_stream_tail_is_open() {
    let mut history = History::new();
    let live = history.push(live_view()).unwrap();
    let stream = history.push_stream(TestSource::new("A", 0, false)).unwrap();

    history.freeze(live, "finished").unwrap();
    assert!(matches!(
        &history.units().next().unwrap().content,
        HistoryUnitContent::Static(_)
    ));
    assert_eq!(history.units().nth(1).unwrap().id, stream.unit());
}

#[test]
fn heterogeneous_non_send_streams_keep_independent_coordinate_spaces() {
    let mut history = History::new();
    let first = history
        .push_stream(TestSource::new("first", 0, false))
        .unwrap();
    history.seal_stream(first).unwrap();
    let second = history
        .push_stream(OtherSource {
            inner: TestSource::new("second", 0, false),
        })
        .unwrap();
    history.seal_stream(second).unwrap();
    let third = history
        .push_stream(TestSource::new("third", 0, false))
        .unwrap();

    assert_ne!(first.unit(), second.unit());
    assert_ne!(second.unit(), third.unit());
    assert_eq!(
        stream_snapshot(&history, first.unit()).source_base(),
        StreamOffset::ZERO
    );
    assert_eq!(
        stream_snapshot(&history, second.unit()).source_base(),
        StreamOffset::ZERO
    );
    assert_eq!(
        unit_ids(&history),
        vec![first.unit(), second.unit(), third.unit()]
    );
}

#[test]
fn stream_refresh_failure_does_not_publish_source_snapshot() {
    let source = TestSource::new("A", 0, false);
    let mut history = History::new();
    let handle = history.push_stream(source).unwrap();
    let before = stream_snapshot(&history, handle.unit());

    history
        .update_stream(handle, |source| {
            let changed = source.snapshot_with(0, 1, "AB");
            source.replace_snapshot(changed);
        })
        .unwrap();
    assert_eq!(
        history.refresh_stream(handle),
        Err(HistoryError::Stream(
            StreamModelError::ChangedWithoutRevision,
        ))
    );
    assert_eq!(stream_snapshot(&history, handle.unit()), before);
}

#[test]
fn sealed_stream_rejects_shared_source_mutation() {
    let source = TestSource::new("A", 0, false);
    let mut history = History::new();
    let handle = history.push_stream(source.clone()).unwrap();
    history.seal_stream(handle).unwrap();
    let before = stream_snapshot(&history, handle.unit());

    source.mutate_after_seal("AB");
    assert_eq!(
        history.refresh_stream(handle),
        Err(HistoryError::SealedStreamChanged {
            unit: handle.unit(),
        })
    );
    assert_eq!(stream_snapshot(&history, handle.unit()), before);
    assert_eq!(
        history.update_stream(handle, |_| ()),
        Err(HistoryError::StreamAlreadySealed {
            unit: handle.unit(),
        })
    );
}

#[test]
fn failed_stream_seal_does_not_publish_model_state() {
    let mut history = History::new();
    let handle = history
        .push_stream(FailingSealSource {
            inner: TestSource::new("AB", 0, false),
        })
        .unwrap();
    let before = stream_snapshot(&history, handle.unit());

    assert_eq!(
        history.seal_stream(handle),
        Err(HistoryError::Stream(StreamModelError::UnstableAfterSeal))
    );
    assert_eq!(stream_snapshot(&history, handle.unit()), before);
}

#[test]
fn sealed_source_must_enter_history_stable() {
    assert_eq!(
        History::new().push_stream(TestSource::new("A", 0, true)),
        Err(HistoryError::Stream(StreamModelError::UnstableAfterSeal))
    );
}

#[test]
fn wrong_stream_handle_type_is_rejected_without_mutation() {
    let mut history = History::new();
    let handle = history.push_stream(TestSource::new("A", 0, false)).unwrap();
    let wrong = HistoryStreamHandle::<OtherSource>::new(handle.unit());
    let before = stream_snapshot(&history, handle.unit());

    assert_eq!(
        history.update_stream(wrong, |_| ()),
        Err(HistoryError::StreamTypeMismatch {
            unit: handle.unit(),
        })
    );
    assert_eq!(stream_snapshot(&history, handle.unit()), before);
}

fn unit_ids(history: &History) -> Vec<HistoryUnitId> {
    history.units().map(|unit| unit.id).collect()
}
