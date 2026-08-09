use crate::presentation::{IntoView, TextSpan, View};
use crate::stream::{
    StreamNode, StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamValidationError,
    StreamView,
};

#[test]
fn structured_snapshot_validation_identifies_a_gap() {
    let snapshot = StreamSnapshot {
        revision: StreamRevision::ZERO,
        source_base: StreamOffset::ZERO,
        source_end: StreamOffset::new(3),
        stable_through: StreamOffset::new(3),
        view: StreamView::new(vec![StreamNode::exact_text(
            StreamRange::new(StreamOffset::new(1), StreamOffset::new(3)),
            vec![TextSpan::plain("ab")],
        )]),
    };
    assert_eq!(
        snapshot.validate(),
        Err(StreamValidationError::FirstNodeDoesNotStartAtBase)
    );
}

#[test]
fn ordinary_atomic_view_is_valid() {
    let snapshot = StreamSnapshot {
        revision: StreamRevision::ZERO,
        source_base: StreamOffset::ZERO,
        source_end: StreamOffset::new(1),
        stable_through: StreamOffset::new(1),
        view: StreamView::new(vec![StreamNode::atomic(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
            View::text("x").into_view(),
        )]),
    };
    assert!(snapshot.validate().is_ok());
}
