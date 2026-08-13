use std::{cell::RefCell, rc::Rc};

use iyon_tui::stream::{
    ProjectedText, StreamOffset, StreamRange, StreamRevision, StreamSnapshot,
    StreamSnapshotBuilder, StreamingSource,
};
use iyon_tui::{IntoView, StreamPane, StyleSpec, TextSpan, View, WrapMode};

struct LocalSource {
    state: Rc<RefCell<(StreamSnapshot, bool)>>,
}

impl StreamingSource for LocalSource {
    fn snapshot(&self) -> StreamSnapshot {
        self.state.borrow().0.clone()
    }

    fn seal(&mut self) {
        self.state.borrow_mut().1 = true;
    }

    fn is_sealed(&self) -> bool {
        self.state.borrow().1
    }
}

fn exact_snapshot(text: &str) -> StreamSnapshot {
    let end = StreamOffset::new(text.len() as u64);
    StreamSnapshotBuilder::new(StreamRevision::ZERO, StreamOffset::ZERO, end, end)
        .exact_text(
            StreamRange::new(StreamOffset::ZERO, end),
            [TextSpan::plain(text)],
        )
        .finish()
        .unwrap()
}

#[test]
fn external_source_can_build_exact_and_projected_snapshots() {
    let projected =
        ProjectedText::builder(StreamRange::new(StreamOffset::ZERO, StreamOffset::new(9)))
            .run(
                "hello",
                StreamRange::new(StreamOffset::ZERO, StreamOffset::new(9)),
                Some(StreamRange::new(StreamOffset::new(2), StreamOffset::new(7))),
                StyleSpec::new().bold(),
            )
            .wrap(WrapMode::Grapheme)
            .finish()
            .unwrap();
    assert_eq!(
        projected.content_range(),
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(9))
    );

    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(9),
        StreamOffset::new(9),
    )
    .projected_text(projected)
    .finish()
    .unwrap();
    assert_eq!(snapshot.source_end(), StreamOffset::new(9));
}

#[test]
fn external_hanging_projection_builder_is_validated() {
    let range = StreamRange::new(StreamOffset::ZERO, StreamOffset::new(6));
    let projected = ProjectedText::builder(range)
        .hanging(
            2,
            "> ",
            StyleSpec::new(),
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(2)),
            true,
        )
        .exact_styled(
            StreamRange::new(StreamOffset::new(2), StreamOffset::new(6)),
            "body",
            StyleSpec::new(),
        )
        .finish()
        .unwrap();
    assert_eq!(projected.content_range(), range);
}

#[test]
fn non_send_source_can_back_a_stream_pane() {
    let state = Rc::new(RefCell::new((exact_snapshot("hello"), false)));
    let mut pane = StreamPane::new(LocalSource {
        state: state.clone(),
    })
    .unwrap();
    pane.refresh().unwrap();
    pane.update_source(|source| {
        let _ = source;
    })
    .unwrap();
    assert!(!pane.is_sealed());
}

#[test]
fn ordinary_atomic_view_is_accepted_by_public_builder() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(1),
        StreamOffset::new(1),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
        View::text("x").into_view(),
    )
    .unwrap()
    .finish()
    .unwrap();
    assert_eq!(snapshot.source_end(), StreamOffset::new(1));
}
