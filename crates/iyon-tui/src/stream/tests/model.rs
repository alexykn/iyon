use std::{cell::RefCell, rc::Rc};

use crate::TextSpan;
use crate::stream::*;

#[derive(Clone)]
struct FakeSource {
    state: Rc<RefCell<FakeState>>,
}

struct FakeState {
    nodes: Vec<StreamNode>,
    stable_through: StreamOffset,
    source_base: StreamOffset,
    revision: StreamRevision,
    sealed: bool,
    corrupt_on_compact: bool,
}

impl FakeSource {
    fn new(stable_through: u64) -> Self {
        Self {
            state: Rc::new(RefCell::new(FakeState {
                nodes: vec![
                    StreamNode::exact_text(range(0, 1), vec![TextSpan::plain("A")]),
                    StreamNode::exact_text(range(1, 2), vec![TextSpan::plain("B")]),
                    StreamNode::exact_text(range(2, 3), vec![TextSpan::plain("C")]),
                ],
                stable_through: StreamOffset::new(stable_through),
                source_base: StreamOffset::ZERO,
                revision: StreamRevision::ZERO,
                sealed: false,
                corrupt_on_compact: false,
            })),
        }
    }

    fn set_stable_without_revision(&self, stable_through: u64) {
        self.state.borrow_mut().stable_through = StreamOffset::new(stable_through);
    }

    fn corrupt_on_compact(&self) {
        self.state.borrow_mut().corrupt_on_compact = true;
    }
}

impl StreamingSource for FakeSource {
    fn snapshot(&self) -> StreamSnapshot {
        let state = self.state.borrow();
        let view = StreamView::new(state.nodes.clone()).suffix_from(state.source_base);
        StreamSnapshot {
            revision: state.revision,
            source_base: state.source_base,
            source_end: StreamOffset::new(3),
            stable_through: state.stable_through,
            view,
        }
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        let mut state = self.state.borrow_mut();
        state.source_base = offset;
        if state.corrupt_on_compact {
            state.nodes[2] = StreamNode::exact_text(range(2, 3), vec![TextSpan::plain("X")]);
        }
        state.revision = state.revision.next();
    }

    fn seal(&mut self) {
        let mut state = self.state.borrow_mut();
        state.sealed = true;
        state.stable_through = StreamOffset::new(3);
        state.revision = state.revision.next();
    }

    fn is_sealed(&self) -> bool {
        self.state.borrow().sealed
    }
}

fn range(start: u64, end: u64) -> StreamRange {
    StreamRange::new(StreamOffset::new(start), StreamOffset::new(end))
}

fn semantic_text(model: &StreamModel<FakeSource>) -> String {
    model
        .semantic_view()
        .nodes
        .iter()
        .filter_map(|node| match node {
            StreamNode::Text(text) => Some(
                text.runs
                    .iter()
                    .map(|run| run.display.as_str())
                    .collect::<String>(),
            ),
            StreamNode::Atomic { .. } => None,
        })
        .collect()
}

#[test]
fn non_send_source_is_accepted_and_resident_capture_is_semantic() {
    let source = FakeSource::new(2);
    let mut model = StreamModel::new(source).unwrap();
    model.refresh().unwrap();
    assert_eq!(model.resident().end(), StreamOffset::new(2));
    assert_eq!(semantic_text(&model), "ABC");

    let wide = model.compile(20);
    let narrow = model.compile(2);
    assert_eq!(
        wide.rows
            .iter()
            .map(|row| row.plain_text())
            .collect::<String>(),
        "ABC"
    );
    assert_eq!(
        narrow
            .rows
            .iter()
            .map(|row| row.plain_text())
            .collect::<String>(),
        "ABC"
    );
}

#[test]
fn resident_source_overlap_does_not_duplicate_nodes() {
    let source = FakeSource::new(2);
    let mut model = StreamModel::new(source).unwrap();
    model.refresh().unwrap();
    assert_eq!(model.semantic_view().nodes.len(), 3);
    assert_eq!(semantic_text(&model), "ABC");
}

#[test]
fn malicious_compaction_semantic_change_is_rejected() {
    let source = FakeSource::new(2);
    source.corrupt_on_compact();
    let mut model = StreamModel::new(source).unwrap();
    assert_eq!(
        model.refresh(),
        Err(StreamModelError::CompactionChangedSemanticSuffix)
    );
}

#[test]
fn changed_snapshot_without_revision_is_rejected() {
    let source = FakeSource::new(1);
    let mut model = StreamModel::new(source.clone()).unwrap();
    source.set_stable_without_revision(2);
    assert_eq!(
        model.refresh(),
        Err(StreamModelError::ChangedWithoutRevision)
    );
}

#[test]
fn sealing_captures_every_whole_node() {
    let source = FakeSource::new(2);
    let mut model = StreamModel::new(source).unwrap();
    model.seal().unwrap();
    assert_eq!(model.snapshot().stable_through, model.snapshot().source_end);
    assert_eq!(model.resident().end(), model.snapshot().source_end);
}

#[test]
fn resident_release_does_not_split_a_node() {
    let source = FakeSource::new(3);
    let mut model = StreamModel::new(source).unwrap();
    model.refresh().unwrap();
    assert_eq!(
        model.release_resident_through(StreamOffset::new(1)),
        StreamOffset::new(1)
    );
    assert_eq!(model.resident().end(), StreamOffset::new(3));
    assert_eq!(semantic_text(&model), "BC");
}

#[test]
fn resident_release_keeps_a_node_whole_when_offset_is_inside_it() {
    let mut resident = ResidentPrefix::new(StreamOffset::ZERO);
    resident.push(StreamNode::exact_text(
        range(0, 1),
        vec![TextSpan::plain("A")],
    ));
    resident.push(StreamNode::exact_text(
        range(1, 3),
        vec![TextSpan::plain("B")],
    ));
    resident.push(StreamNode::exact_text(
        range(3, 4),
        vec![TextSpan::plain("C")],
    ));

    assert_eq!(
        resident.release_through(StreamOffset::new(2)),
        StreamOffset::new(1)
    );
    assert_eq!(resident.nodes().count(), 2);
    assert_eq!(
        resident.release_through(StreamOffset::new(3)),
        StreamOffset::new(3)
    );
    assert_eq!(resident.nodes().count(), 1);
}
