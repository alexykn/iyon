use crate::stream::StreamSliceError;
use crate::stream::{
    StreamNode, StreamOffset, StreamRange, StreamRowAnchor, StreamRowTransfer, StreamView,
    compile_stream,
};
use crate::{IntoView, View};

#[test]
fn exact_multiline_rows_anchor_after_hard_newline() {
    let end = StreamOffset::new(5);
    let view = StreamView::exact_text(
        StreamRange::new(StreamOffset::ZERO, end),
        vec![crate::TextSpan::plain("A\nB\nC")],
    );
    let compiled = compile_stream(&view, 20, end);
    assert_eq!(
        compiled
            .rows
            .iter()
            .map(|row| row.anchor.clone())
            .collect::<Vec<_>>(),
        vec![
            StreamRowAnchor::Checkpoint(StreamOffset::ZERO),
            StreamRowAnchor::Checkpoint(StreamOffset::new(2)),
            StreamRowAnchor::Checkpoint(StreamOffset::new(4)),
        ]
    );
}

#[test]
fn empty_hard_lines_keep_consecutive_source_anchors() {
    for (source, expected) in [
        ("\nB", vec![0, 1]),
        ("\n\nB", vec![0, 1, 2]),
        ("A\n\nB", vec![0, 2, 3]),
        ("A\n\n", vec![0, 2, 3]),
    ] {
        let end = StreamOffset::new(source.len() as u64);
        let compiled = compile_stream(
            &StreamView::exact_text(
                StreamRange::new(StreamOffset::ZERO, end),
                vec![crate::TextSpan::plain(source)],
            ),
            20,
            end,
        );
        let anchors = compiled
            .rows
            .iter()
            .map(|row| match row.anchor {
                StreamRowAnchor::Checkpoint(offset) => offset.as_u64(),
                StreamRowAnchor::Atomic { .. } => panic!("text rows cannot be Atomic"),
            })
            .collect::<Vec<_>>();
        assert_eq!(anchors, expected, "source={source:?}");
    }
}

#[test]
fn multiline_rows_transfer_at_newline_aware_boundaries() {
    for (source, expected) in [
        ("A\nB", vec![2, 3]),
        ("\nB", vec![1, 2]),
        ("\n\nB", vec![1, 2, 3]),
        ("A\n\nB", vec![2, 3, 4]),
    ] {
        let end = StreamOffset::new(source.len() as u64);
        let compiled = compile_stream(
            &StreamView::exact_text(
                StreamRange::new(StreamOffset::ZERO, end),
                vec![crate::TextSpan::plain(source)],
            ),
            20,
            end,
        );
        let transfers = compiled
            .rows
            .iter()
            .map(|row| match row.transfer {
                StreamRowTransfer::Checkpoint(offset) => offset.as_u64(),
                _ => panic!("stable exact text should transfer by checkpoint"),
            })
            .collect::<Vec<_>>();
        assert_eq!(transfers, expected, "source={source:?}");
    }
}

#[test]
fn wrapped_hard_newline_rows_transfer_at_exact_checkpoints() {
    let source = "abcdefgh\nx";
    let end = StreamOffset::new(source.len() as u64);
    let view = StreamView::exact_text(
        StreamRange::new(StreamOffset::ZERO, end),
        vec![crate::TextSpan::plain(source)],
    );
    let compiled = compile_stream(&view, 4, end);
    assert_eq!(compiled.rows.len(), 3);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(4))
    );
    assert_eq!(
        compiled.rows[1].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(9))
    );
    assert_eq!(
        compiled.rows[2].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(10))
    );
}

#[test]
fn completed_rows_transfer_ahead_of_mutable_tail() {
    let view = StreamView::new(vec![
        StreamNode::exact_line(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(5)),
            vec![crate::TextSpan::plain("done\n")],
            true,
        ),
        StreamNode::exact_text(
            StreamRange::new(StreamOffset::new(5), StreamOffset::new(12)),
            vec![crate::TextSpan::plain("mutable")],
        ),
    ]);
    let compiled = compile_stream(&view, 80, StreamOffset::new(5));
    assert_eq!(compiled.transferable_prefix_rows, 1);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(5))
    );
    assert!(matches!(
        compiled.rows[1].transfer,
        StreamRowTransfer::Blocked
    ));
}

#[test]
fn semantic_slice_respects_text_and_atomic_boundaries() {
    let text = StreamView::exact_text(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        vec![crate::TextSpan::plain("abc")],
    );
    assert!(
        text.semantic_slice(StreamRange::new(StreamOffset::new(1), StreamOffset::new(3)))
            .is_ok()
    );

    let egc = StreamView::exact_text(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(4)),
        vec![crate::TextSpan::plain("👩")],
    );
    assert_eq!(
        egc.semantic_slice(StreamRange::new(StreamOffset::new(1), StreamOffset::new(4))),
        Err(StreamSliceError::IllegalCheckpoint)
    );

    let atomic = StreamView::atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        View::text("atomic").into_view(),
    );
    assert_eq!(
        atomic.semantic_slice(StreamRange::new(StreamOffset::new(1), StreamOffset::new(3))),
        Err(StreamSliceError::AtomicBoundary)
    );
}

#[test]
fn atomic_rows_have_only_atomic_anchors() {
    let view = StreamView::atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        View::vertical(|column| {
            column.children(["a", "b", "c"]);
        }),
    );
    let compiled = compile_stream(&view, 20, StreamOffset::new(3));
    assert!(compiled.rows.iter().all(|row| matches!(
        row.anchor,
        StreamRowAnchor::Atomic { range, .. } if range == StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3))
    )));
}

#[test]
fn empty_hard_newline_transfers_as_one_row() {
    let view = StreamView::new(vec![StreamNode::exact_line(
        StreamRange::new(StreamOffset::new(2), StreamOffset::new(2)),
        Vec::new(),
        true,
    )]);
    let compiled = compile_stream(&view, 20, StreamOffset::new(3));
    assert_eq!(compiled.rows.len(), 1);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(3))
    );
}
