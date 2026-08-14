use std::{cell::RefCell, rc::Rc};

use crate::{
    TextSpan,
    stream::{
        StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamSnapshotBuilder,
        StreamingSource,
    },
};

mod combined;
mod model;
mod projection;
mod stream;

#[derive(Clone)]
struct TestSource {
    state: Rc<RefCell<TestSourceState>>,
}

struct TestSourceState {
    snapshot: StreamSnapshot,
    sealed: bool,
    compact_calls: usize,
}

impl TestSource {
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

    fn from_snapshot(snapshot: StreamSnapshot, sealed: bool) -> Self {
        Self {
            state: Rc::new(RefCell::new(TestSourceState {
                snapshot,
                sealed,
                compact_calls: 0,
            })),
        }
    }

    fn compact_calls(&self) -> usize {
        self.state.borrow().compact_calls
    }

    fn replace_snapshot(&self, snapshot: StreamSnapshot) {
        self.state.borrow_mut().snapshot = snapshot;
    }

    fn snapshot_with(&self, revision: u64, stable_through: u64, text: &str) -> StreamSnapshot {
        let end = text.len() as u64;
        StreamSnapshotBuilder::new(
            StreamRevision::new(revision),
            StreamOffset::ZERO,
            StreamOffset::new(stable_through),
            StreamOffset::new(end),
        )
        .exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(end)),
            [TextSpan::plain(text)],
        )
        .finish()
        .unwrap()
    }

    fn mutate_after_seal(&self, text: &str) {
        let snapshot = self.snapshot_with(2, text.len() as u64, text);
        self.replace_snapshot(snapshot);
    }
}

impl StreamingSource for TestSource {
    fn snapshot(&self) -> StreamSnapshot {
        self.state.borrow().snapshot.clone()
    }

    fn seal(&mut self) {
        let mut state = self.state.borrow_mut();
        state.sealed = true;
        state.snapshot.revision = StreamRevision::new(state.snapshot.revision().as_u64() + 1);
        state.snapshot.stable_through = state.snapshot.source_end;
    }

    fn is_sealed(&self) -> bool {
        self.state.borrow().sealed
    }
}
