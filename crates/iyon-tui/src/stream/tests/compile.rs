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

#[test]
fn atomic_beyond_stable_through_is_blocked_from_native_transfer() {
    let table = StreamRange::new(StreamOffset::new(5), StreamOffset::new(20));
    let view = StreamView::new(vec![
        StreamNode::exact_text(
            StreamRange::new(StreamOffset::ZERO, StreamOffset::new(5)),
            vec![crate::TextSpan::plain("intro")],
        ),
        StreamNode::atomic(table, View::text("| A | B |\n| 1 | 2 |").into_view()),
    ]);
    let open = compile_stream(&view, 40, StreamOffset::new(5));
    assert_eq!(open.transferable_prefix_rows, 1);
    assert!(
        open.rows
            .iter()
            .filter(
                |row| matches!(row.anchor, StreamRowAnchor::Atomic { range, .. } if range == table)
            )
            .all(|row| matches!(row.transfer, StreamRowTransfer::Blocked)),
        "raw table rows must not transfer while the range is still revisable"
    );

    let closed = compile_stream(&view, 40, table.end());
    assert!(
        closed.rows.iter().any(|row| matches!(
            row.transfer,
            StreamRowTransfer::Atomic { source_end, .. } if source_end == table.end()
        )),
        "the same range may transfer only after it is stable"
    );
}

#[test]
fn stable_clipped_atomic_is_transferable() {
    let range = StreamRange::new(StreamOffset::ZERO, StreamOffset::new(10));
    let view = StreamView::atomic(
        range,
        View::text("abcdefghijklmnopqrstuvwxyz")
            .no_wrap()
            .into_view(),
    );
    let compiled = compile_stream(&view, 5, range.end());
    assert_eq!(compiled.rows.len(), 1);
    assert!(compiled.rows[0].physical.width() <= 5);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Atomic {
            group: crate::stream::StreamAtomicId(1),
            source_end: range.end(),
        }
    );
    assert_eq!(compiled.transferable_prefix_rows, 1);

    let compiler = crate::presentation::layout::ViewCompiler::new(&crate::Theme::default());
    let layout = compiler.compile(
        &View::text("abcdefghijklmnopqrstuvwxyz")
            .no_wrap()
            .into_view(),
        5,
    );
    assert!(!layout.physically_complete);
}

#[test]
fn unstable_clipped_atomic_remains_blocked() {
    let range = StreamRange::new(StreamOffset::ZERO, StreamOffset::new(10));
    let view = StreamView::atomic(
        range,
        View::text("abcdefghijklmnopqrstuvwxyz")
            .no_wrap()
            .into_view(),
    );
    let compiled = compile_stream(&view, 5, StreamOffset::new(5));
    assert_eq!(compiled.rows.len(), 1);
    assert_eq!(compiled.rows[0].transfer, StreamRowTransfer::Blocked);
    assert_eq!(compiled.transferable_prefix_rows, 0);
}

#[test]
fn stable_wide_grapheme_at_width_one_is_transferable() {
    for (source, expected_width) in [("🐕‍🦺", 2), ("👩‍🔬", 2), ("🇮🇩", 2), ("e\u{301}", 1)]
    {
        let end = StreamOffset::new(source.len() as u64);
        assert_eq!(crate::physical::grapheme_cell_width(source), expected_width);
        let view = StreamView::exact_text(
            StreamRange::new(StreamOffset::ZERO, end),
            vec![crate::TextSpan::plain(source)],
        );
        let compiled = compile_stream(&view, 1, end);

        assert_eq!(compiled.rows.len(), 1, "source={source:?}");
        assert_eq!(compiled.transferable_prefix_rows, 1, "source={source:?}");
        assert_eq!(
            compiled.rows[0].transfer,
            StreamRowTransfer::Checkpoint(end),
            "source={source:?}"
        );
        assert!(
            compiled
                .rows
                .iter()
                .all(|row| row.physical.validate_cell_geometry().is_ok()),
            "source={source:?} produced invalid physical geometry"
        );
        assert!(
            compiled.rows[0]
                .physical
                .cells()
                .iter()
                .enumerate()
                .all(|(column, cell)| !cell.continuation || column > 0),
            "source={source:?} produced an orphan continuation"
        );
        assert_eq!(
            compiled.rows[0].physical.width(),
            expected_width,
            "source={source:?} must remain one complete EGC"
        );
        let placed = compiled.rows[0].physical.placed(1, 0);
        assert!(
            placed.validate_cell_geometry().is_ok(),
            "source={source:?} produced invalid placed geometry"
        );
        assert_eq!(
            placed.cells().iter().filter(|cell| cell.painted).count(),
            usize::from(expected_width == 1),
            "source={source:?} must be painted atomically at viewport width one"
        );
        assert!(
            placed.cells().iter().all(|cell| !cell.continuation),
            "source={source:?} must not leave an orphan continuation after placement"
        );
    }
}

#[test]
fn stable_clipped_atomic_does_not_block_following_node() {
    let atomic_range = StreamRange::new(StreamOffset::ZERO, StreamOffset::new(26));
    let after_range = StreamRange::new(StreamOffset::new(26), StreamOffset::new(31));
    let view = StreamView::new(vec![
        StreamNode::atomic(
            atomic_range,
            View::text("abcdefghijklmnopqrstuvwxyz")
                .no_wrap()
                .into_view(),
        ),
        StreamNode::exact_text(after_range, vec![crate::TextSpan::plain("AFTER")]),
    ]);
    let compiled = compile_stream(&view, 5, after_range.end());
    assert_eq!(compiled.rows.len(), 2);
    assert_eq!(compiled.transferable_prefix_rows, 2);
    assert!(matches!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Atomic { .. }
    ));
    assert_eq!(
        compiled.rows[1].transfer,
        StreamRowTransfer::Checkpoint(after_range.end())
    );
}
