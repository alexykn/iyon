use super::*;
use crate::{
    StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamSnapshotBuilder,
    StreamValidationError, TextSpan,
    geometry::Size,
    presentation::{IntoView, layout::ViewCompiler},
};

struct TestSource {
    snapshot: StreamSnapshot,
    sealed: bool,
}

impl StreamingSource for TestSource {
    fn snapshot(&self) -> StreamSnapshot {
        self.snapshot.clone()
    }

    fn seal(&mut self) {
        self.sealed = true;
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }
}

struct CompactingSource {
    snapshot: StreamSnapshot,
    appended: bool,
}

impl StreamingSource for CompactingSource {
    fn snapshot(&self) -> StreamSnapshot {
        self.snapshot.clone()
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        let end = self.snapshot.source_end();
        self.snapshot = StreamSnapshotBuilder::new(
            self.snapshot.revision().next(),
            offset,
            self.snapshot.stable_through(),
            end,
        )
        .finish()
        .unwrap();
    }

    fn seal(&mut self) {}

    fn is_sealed(&self) -> bool {
        false
    }
}

fn snapshot(revision: u64, text: &str) -> StreamSnapshot {
    let end = StreamOffset::new(text.len() as u64);
    StreamSnapshotBuilder::new(StreamRevision::new(revision), StreamOffset::ZERO, end, end)
        .exact_text(
            StreamRange::new(StreamOffset::ZERO, end),
            [TextSpan::plain(text)],
        )
        .finish()
        .unwrap()
}

fn rows<S: StreamingSource>(pane: &StreamPane<S>) -> Vec<String> {
    let size = pane.layout_size.unwrap_or(Size::new(4, 2));
    rows_at(pane, size.width, size.height)
}

fn rows_at<S: StreamingSource>(pane: &StreamPane<S>, width: u16, height: u16) -> Vec<String> {
    ViewCompiler::default()
        .compile_bounded(&pane.view(), Size::new(width, height))
        .rows
        .iter()
        .map(|row| row.plain_text())
        .collect()
}

#[test]
fn follow_end_and_detached_checkpoint_behavior() {
    let mut pane = StreamPane::new(TestSource {
        snapshot: snapshot(0, "A\nB\nC"),
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(4, 2));
    assert_eq!(rows(&pane), ["B", "C"]);

    assert!(pane.scroll_up(1));
    assert!(!pane.is_following_end());
    assert_eq!(rows(&pane), ["A", "B"]);

    pane.update_source(|source| {
        source.snapshot = snapshot(1, "A\nB\nC\nD");
    })
    .unwrap();
    assert_eq!(rows(&pane), ["A", "B"]);

    pane.follow_end();
    assert_eq!(rows(&pane), ["C", "D"]);
}

#[test]
fn follow_end_renders_after_consecutive_empty_hard_lines() {
    for text in ["\nB", "\n\nB", "A\n\nB", "A\n\n"] {
        let mut pane = StreamPane::new(TestSource {
            snapshot: snapshot(0, text),
            sealed: false,
        })
        .unwrap();
        pane.on_layout_changed(Size::new(8, 1));
        let rendered = rows(&pane);
        let expected = text.rsplit('\n').next().unwrap_or_default();
        assert_eq!(rendered, [expected], "source={text:?}");
    }
}

#[test]
fn atomic_subrow_anchor_uses_no_fake_checkpoint() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(3),
        StreamOffset::new(3),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        View::vertical(|column| {
            column.children(["one", "two", "three"]);
        }),
    )
    .unwrap()
    .finish()
    .unwrap();
    let mut pane = StreamPane::new(TestSource {
        snapshot,
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(8, 1));
    assert_eq!(rows_at(&pane, 8, 1), ["three"]);
    assert!(pane.scroll_up(1));
    assert_eq!(rows_at(&pane, 8, 1), ["two"]);
    assert!(pane.scroll_up(1));
    assert_eq!(rows_at(&pane, 8, 1), ["one"]);
}

#[test]
fn atomic_resize_preserves_subrow_anchor() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(3),
        StreamOffset::new(3),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        View::vertical(|column| {
            column.children(["one", "two", "three"]);
        }),
    )
    .unwrap()
    .finish()
    .unwrap();
    let mut pane = StreamPane::new(TestSource {
        snapshot,
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(Size::new(8, 1));
    pane.scroll_up(1);
    pane.on_layout_changed(Size::new(8, 2));
    assert_eq!(rows(&pane), ["two", "three"]);
}

#[test]
fn atomic_width_reflow_repairs_disappearing_subrow() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(1),
        StreamOffset::new(1),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
        View::text("abcdefghijkl").into_view(),
    )
    .unwrap()
    .finish()
    .unwrap();
    let mut pane = StreamPane::new(TestSource {
        snapshot,
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(Size::new(4, 1));
    assert!(pane.scroll_up(1));
    assert!(!pane.is_following_end());
    pane.on_layout_changed(Size::new(12, 1));
    assert!(!pane.is_following_end());
    assert_eq!(rows(&pane), ["abcdefghijkl"]);
}

#[test]
fn resident_compaction_keeps_old_content_scrollable() {
    let initial = snapshot(0, "A\nB\nC");
    let mut pane = StreamPane::new(CompactingSource {
        snapshot: initial,
        appended: false,
    })
    .unwrap();
    pane.refresh().unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(4, 2));
    pane.scroll_to_start();
    assert_eq!(rows(&pane), ["A", "B"]);

    pane.update_source(|source| {
        let start = StreamOffset::new(5);
        let end = StreamOffset::new(7);
        source.snapshot = StreamSnapshotBuilder::new(StreamRevision::new(2), start, end, end)
            .exact_text(StreamRange::new(start, end), [TextSpan::plain("\nD")])
            .finish()
            .unwrap();
        source.appended = true;
    })
    .unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(8, 1));
    assert_eq!(rows(&pane), ["A"]);
}

#[test]
fn page_scrolling_moves_by_viewport_and_restores_follow_end() {
    let mut pane = StreamPane::new(TestSource {
        snapshot: snapshot(0, "0\n1\n2\n3\n4\n5"),
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(Size::new(4, 2));
    assert_eq!(rows(&pane), ["4", "5"]);
    assert!(pane.page_up());
    assert_eq!(rows(&pane), ["2", "3"]);
    assert!(!pane.is_following_end());
    assert!(pane.page_down());
    assert!(pane.is_following_end());
    assert_eq!(rows(&pane), ["4", "5"]);
}

#[test]
fn stable_detached_anchor_and_height_growth_preserve_top_row() {
    let mut pane = StreamPane::new(TestSource {
        snapshot: snapshot(0, "A\nB\nC\nD"),
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(Size::new(4, 1));
    pane.scroll_to_start();
    let anchor = pane.mode.clone();
    pane.on_layout_changed(Size::new(4, 3));
    assert_eq!(pane.mode, anchor);
    assert_eq!(rows(&pane), ["A", "B", "C"]);
}

#[test]
fn zero_width_scrolling_preserves_detached_anchor() {
    let mut pane = StreamPane::new(TestSource {
        snapshot: snapshot(0, "A\nB\nC"),
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(Size::new(4, 1));
    pane.scroll_up(1);
    let mode = pane.mode.clone();
    pane.on_layout_changed(Size::new(0, 5));
    assert_eq!(pane.mode, mode);
    assert!(!pane.scroll_up(1));
    assert!(!pane.scroll_down(1));
    assert!(!pane.page_up());
    assert!(!pane.page_down());
    pane.scroll_to_start();
    assert_eq!(pane.mode, mode);
    pane.follow_end();
    assert!(pane.is_following_end());
}

#[test]
fn private_row_viewport_crops_rows_without_overflow_content() {
    let view = View::row_viewport(
        View::vertical(|column| {
            column.children(["one", "two", "three"]);
        }),
        1,
    );
    let compiled = ViewCompiler::default().compile_bounded(&view, Size::new(8, 1));
    assert_eq!(compiled.rows.len(), 1);
    assert_eq!(compiled.rows[0].plain_text(), "two");
}

#[test]
fn unstable_checkpoint_repairs_without_returning_to_follow_end() {
    let mut pane = StreamPane::new(TestSource {
        snapshot: {
            let end = StreamOffset::new(1);
            StreamSnapshotBuilder::new(
                StreamRevision::ZERO,
                StreamOffset::ZERO,
                StreamOffset::ZERO,
                end,
            )
            .exact_text(
                StreamRange::new(StreamOffset::ZERO, end),
                [TextSpan::plain("a")],
            )
            .finish()
            .unwrap()
        },
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(4, 1));
    pane.scroll_to_start();
    pane.update_source(|source| {
        let end = StreamOffset::new(3);
        source.snapshot =
            StreamSnapshotBuilder::new(StreamRevision::new(1), StreamOffset::ZERO, end, end)
                .exact_text(
                    StreamRange::new(StreamOffset::ZERO, end),
                    [TextSpan::plain("a\u{301}")],
                )
                .finish()
                .unwrap();
    })
    .unwrap();
    assert!(!pane.is_following_end());
    assert_eq!(rows(&pane), ["a\u{301}"]);
}

#[test]
fn replaced_atomic_anchor_repairs_near_its_source_start() {
    let initial = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(3),
        StreamOffset::new(3),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        View::vertical(|column| {
            column.children(["a", "b", "c"]);
        }),
    )
    .unwrap()
    .finish()
    .unwrap();
    let mut pane = StreamPane::new(TestSource {
        snapshot: initial,
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(4, 1));
    assert!(pane.scroll_up(1));
    pane.update_source(|source| {
        let range = StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3));
        let projected = crate::ProjectedText::builder(range)
            .replacement("x", range, crate::StyleSpec::new())
            .finish()
            .unwrap();
        source.snapshot = StreamSnapshotBuilder::new(
            StreamRevision::new(1),
            StreamOffset::ZERO,
            StreamOffset::new(3),
            StreamOffset::new(3),
        )
        .projected_text(projected)
        .finish()
        .unwrap();
    })
    .unwrap();
    assert!(!pane.is_following_end());
    assert_eq!(rows(&pane), ["x"]);
}

#[test]
fn detached_checkpoint_survives_width_reflow() {
    let mut pane = StreamPane::new(TestSource {
        snapshot: snapshot(0, "abcdef"),
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(3, 1));
    pane.scroll_to_start();
    pane.on_layout_changed(crate::geometry::Size::new(6, 1));
    assert!(!pane.is_following_end());
    assert_eq!(rows(&pane), ["abcdef"]);
}

#[test]
fn public_atomic_builder_rejects_component_identity() {
    let view = View::text("x")
        .into_view()
        .with_component(crate::component::ComponentId::allocate());
    let result = StreamSnapshotBuilder::new(
        StreamRevision::ZERO,
        StreamOffset::ZERO,
        StreamOffset::new(1),
        StreamOffset::new(1),
    )
    .atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
        view,
    );
    assert!(matches!(
        result,
        Err(StreamValidationError::AtomicContainsComponent)
    ));
}

#[test]
fn zero_height_scrolling_is_safe() {
    let mut pane = StreamPane::new(TestSource {
        snapshot: snapshot(0, "A\nB"),
        sealed: false,
    })
    .unwrap();
    pane.on_layout_changed(crate::geometry::Size::new(4, 0));
    assert!(!pane.scroll_up(1));
    assert!(!pane.scroll_down(1));
    assert!(pane.is_following_end());
}
