use super::*;
use crate::{
    IntoView, OverflowIndicator, TextSpan, View, physical::PhysicalRow,
    presentation::layout::ViewCompiler, stream::*,
};

fn assert_rows_equivalent(left: &[PhysicalRow], right: &[PhysicalRow]) {
    assert_eq!(left.len(), right.len());
    for (left, right) in left.iter().zip(right) {
        assert_eq!(left.plain_text().trim_end(), right.plain_text().trim_end());
    }
}

fn range(start: u64, end: u64) -> StreamRange {
    StreamRange::new(StreamOffset::new(start), StreamOffset::new(end))
}

#[derive(Debug)]
struct MockStream {
    source: String,
    stable_len: usize,
    sealed: bool,
    revision: StreamRevision,
}

impl StreamingSource for MockStream {
    fn snapshot(&self) -> StreamSnapshot {
        let len = self.source.len() as u64;
        let stable = if self.sealed {
            len
        } else {
            self.stable_len as u64
        };
        StreamSnapshot {
            revision: self.revision,
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::new(len),
            stable_through: StreamOffset::new(stable),
            view: StreamView::exact_text(
                StreamRange::new(StreamOffset::ZERO, StreamOffset::new(len)),
                vec![TextSpan::plain(&self.source)],
            ),
        }
    }

    fn seal(&mut self) {
        if self.sealed {
            return;
        }
        self.sealed = true;
        self.revision = self.revision.next();
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }
}

#[derive(Debug)]
struct AtomicMockStream {
    source_base: StreamOffset,
    sealed: bool,
    revision: StreamRevision,
}

impl StreamingSource for AtomicMockStream {
    fn snapshot(&self) -> StreamSnapshot {
        let source_end = StreamOffset::new(42);
        let suffix = StreamNode::exact_text(
            StreamRange::new(StreamOffset::new(30), source_end),
            vec![TextSpan::plain("suffix-after")],
        );
        let view = if self.source_base < StreamOffset::new(30) {
            StreamView::new(vec![
                StreamNode::atomic(
                    StreamRange::new(StreamOffset::ZERO, StreamOffset::new(30)),
                    View::column(
                        vec![
                            View::text("A0").into_view(),
                            View::text("A1").into_view(),
                            View::text("A2").into_view(),
                        ],
                        0,
                    ),
                ),
                suffix,
            ])
        } else if self.source_base < source_end {
            StreamView::new(vec![suffix])
        } else {
            StreamView::empty()
        };

        StreamSnapshot {
            revision: self.revision,
            source_base: self.source_base,
            source_end,
            stable_through: source_end,
            view,
        }
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        self.source_base = offset;
        self.revision = self.revision.next();
    }

    fn seal(&mut self) {
        self.sealed = true;
        self.revision = self.revision.next();
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }
}

#[test]
fn multiline_transfer_checkpoint_does_not_recreate_newline_row() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(43),
        MockStream {
            source: "A\nB".to_string(),
            stable_len: 3,
            sealed: false,
            revision: StreamRevision::ZERO,
        },
        LeadingBoundaryState::None,
    );

    let prepared = hosted.prepare_frame(20, 1);
    assert_eq!(prepared.history.rows.len(), 1);
    assert_eq!(prepared.history.rows.as_slice()[0].plain_text(), "A");
    assert_eq!(
        prepared.history.semantic_plan.next_committed_through,
        StreamOffset::new(2)
    );
    hosted.apply_commit_success(prepared.history);

    let next = hosted.prepare_frame(20, 1);
    assert_eq!(hosted.committed_through, StreamOffset::new(2));
    assert_eq!(next.live_rows[0].plain_text(), "B");
}

#[test]
fn plan_stream_transfer_skips_fully_committed_atomic_group() {
    // Test A: atomic 3 physical rows, commit all 3, plan again from same compiled object => 0 rows
    let atomic_view = View::column(
        vec![
            View::text("heading").into_view(),
            View::text("body row 1").into_view(),
            View::text("body row 2").into_view(),
        ],
        0,
    );

    let view = StreamView::atomic(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(30)),
        atomic_view,
    );

    let compiled = compile_stream(&view, 20, StreamOffset::new(30));
    assert_eq!(compiled.rows.len(), 3);
    assert_eq!(compiled.transferable_prefix_rows, 3);

    // First commit: commit all 3 rows
    let plan1 = plan_stream_transfer(&compiled, 3, None, StreamOffset::ZERO);
    assert_eq!(plan1.payload.rows_to_write(), 3);
    assert_eq!(plan1.next_committed_through, StreamOffset::new(30));
    assert_eq!(plan1.next_partial, None);

    // Plan again from same compiled object without compacting
    let plan2 = plan_stream_transfer(&compiled, 3, None, StreamOffset::new(30));
    assert_eq!(plan2.payload.rows_to_write(), 0);
    assert_eq!(plan2.next_committed_through, StreamOffset::new(30));
}

#[test]
fn plan_stream_transfer_mixed_provenance_skips_exact_and_atomic_groups() {
    // Test B: Exact 0..5, Atomic 5..20, Exact 20..24, commit through 20, plan again
    let mut view = StreamView::empty();
    view.push(StreamNode::exact_text(
        StreamRange::new(StreamOffset::new(0), StreamOffset::new(5)),
        vec![TextSpan::plain("first")],
    ));
    view.push(StreamNode::atomic(
        StreamRange::new(StreamOffset::new(5), StreamOffset::new(20)),
        View::column(
            vec![
                View::text("mid 1").into_view(),
                View::text("mid 2").into_view(),
            ],
            0,
        ),
    ));
    view.push(StreamNode::exact_text(
        StreamRange::new(StreamOffset::new(20), StreamOffset::new(24)),
        vec![TextSpan::plain("last")],
    ));

    let compiled = compile_stream(&view, 20, StreamOffset::new(24));
    assert_eq!(compiled.rows.len(), 4);

    // Commit through source 20 (skips first Exact and whole Atomic group)
    let plan = plan_stream_transfer(&compiled, 10, None, StreamOffset::new(20));
    // Only the final Exact row is written
    assert_eq!(plan.payload.rows_to_write(), 1);
    assert_eq!(plan.next_committed_through, StreamOffset::new(24));
}

#[test]
fn compile_stream_atomic_partial_freezes_physical_rows() {
    let atomic_view = View::column(
        vec![
            View::text("heading").into_view(),
            View::text("body row 1").into_view(),
            View::text("body row 2").into_view(),
        ],
        0,
    );

    let view = StreamView::atomic(
        StreamRange::new(StreamOffset::new(0), StreamOffset::new(30)),
        atomic_view,
    );

    let compiled = compile_stream(&view, 20, StreamOffset::new(30));
    assert_eq!(compiled.rows.len(), 3);
    assert_eq!(compiled.transferable_prefix_rows, 3);

    // Request 1 row of the 3-row atomic group
    let plan = plan_stream_transfer(&compiled, 1, None, StreamOffset::new(0));
    assert_eq!(plan.payload.rows_to_write(), 1);
    // Source offset does not advance yet!
    assert_eq!(plan.next_committed_through, StreamOffset::new(0));
    assert!(matches!(
        plan.next_partial,
        Some(StreamPartialCommit::FrozenAtomic {
            committed_rows: 1,
            source_end,
            ..
        }) if source_end == StreamOffset::new(30)
    ));

    // Commit remaining 2 rows from partial state
    let plan2 = plan_stream_transfer(
        &compiled,
        2,
        plan.next_partial.as_ref(),
        StreamOffset::new(0),
    );
    assert_eq!(plan2.payload.rows_to_write(), 2);
    assert_eq!(plan2.next_committed_through, StreamOffset::new(30));
    assert_eq!(plan2.next_partial, None);
}

fn frozen_atomic_live_projection_survives_resize() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(42),
        AtomicMockStream {
            source_base: StreamOffset::ZERO,
            sealed: false,
            revision: StreamRevision::ZERO,
        },
        LeadingBoundaryState::None,
    );

    let prepared1 = hosted.prepare_frame(10, 1);
    assert!(prepared1.live_rows.len() >= 4);
    assert_eq!(prepared1.history.semantic_plan.payload.rows_to_write(), 1);
    assert_eq!(
        prepared1.history.semantic_plan.next_committed_through,
        StreamOffset::ZERO
    );
    hosted.apply_commit_success(prepared1.history);

    let (frozen_rows, committed_rows, source_end) = match hosted.partial.as_ref() {
        Some(StreamPartialCommit::FrozenAtomic {
            rows,
            committed_rows,
            source_end,
            ..
        }) => (rows.clone(), *committed_rows, *source_end),
        other => panic!("expected FrozenAtomic partial, got {other:?}"),
    };
    assert_eq!(committed_rows, 1);
    assert_eq!(hosted.committed_through, StreamOffset::ZERO);

    hosted.seal();
    let prepared2 = hosted.prepare_frame(80, 10);
    assert_eq!(&prepared2.live_rows[..2], &frozen_rows.as_slice()[1..]);
    assert_eq!(prepared2.live_rows[2].plain_text(), "suffix-after");
    assert!(matches!(
        prepared2.history.semantic_plan.payload,
        StreamTransferPayload::Frozen { .. }
    ));
    hosted.apply_commit_success(prepared2.history);

    assert_eq!(hosted.committed_through, source_end);
    assert_eq!(hosted.partial, None);
    assert_eq!(hosted.snapshot().source_base, source_end);

    let prepared3 = hosted.prepare_frame(80, 10);
    assert_eq!(
        prepared3.history.semantic_plan.next_committed_through,
        StreamOffset::new(42)
    );
    hosted.apply_commit_success(prepared3.history);
    assert!(hosted.can_handoff());
}

#[test]
fn leading_boundary_commits_with_first_atomic_body_row() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(7),
        AtomicMockStream {
            source_base: StreamOffset::ZERO,
            sealed: false,
            revision: StreamRevision::ZERO,
        },
        LeadingBoundaryState::Pending,
    );

    let prepared = hosted.prepare_frame(80, 2);
    assert_eq!(prepared.history.rows.len(), 2);
    assert!(prepared.history.rows.as_slice()[0].plain_text().is_empty());
    assert_eq!(prepared.history.rows.as_slice()[1].plain_text(), "A0");
    assert!(prepared.history.commit_leading_boundary);
    hosted.apply_commit_success(prepared.history);

    assert_eq!(hosted.leading_boundary, LeadingBoundaryState::Committed);
    assert_eq!(hosted.committed_through, StreamOffset::ZERO);
    assert!(matches!(
        hosted.partial,
        Some(StreamPartialCommit::FrozenAtomic {
            committed_rows: 1,
            ..
        })
    ));

    let reprepared = hosted.prepare_frame(20, 0);
    assert_eq!(reprepared.live_rows[0].plain_text(), "A1");
    assert_ne!(reprepared.live_rows[0], PhysicalRow::empty());
    assert!(!reprepared.history.commit_leading_boundary);
}

#[test]
fn first_hosted_unit_has_no_leading_boundary_transaction() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(8),
        MockStream {
            source: "hello world".to_string(),
            stable_len: 11,
            sealed: false,
            revision: StreamRevision::ZERO,
        },
        LeadingBoundaryState::None,
    );

    let prepared = hosted.prepare_frame(80, 1);
    assert_eq!(prepared.history.rows.len(), 1);
    assert!(!prepared.history.commit_leading_boundary);
    assert_ne!(prepared.history.rows.as_slice()[0], PhysicalRow::empty());
}

#[test]
fn leading_boundary_failure_is_transactional() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(10),
        MockStream {
            source: "hello world".to_string(),
            stable_len: 11,
            sealed: false,
            revision: StreamRevision::ZERO,
        },
        LeadingBoundaryState::Pending,
    );

    let prepared = hosted.prepare_frame(6, 2);
    let expected_rows = prepared.history.rows.clone();
    // Simulate terminal failure by dropping the prepared transaction.
    let reprepared = hosted.prepare_frame(6, 2);
    assert_eq!(reprepared.history.rows, expected_rows);
    assert_eq!(hosted.leading_boundary, LeadingBoundaryState::Pending);
    assert_eq!(hosted.committed_through, StreamOffset::ZERO);
    assert!(hosted.partial.is_none());
}

#[test]
fn leading_boundary_exact_first_spill_is_committed_once() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(9),
        MockStream {
            source: "hello world".to_string(),
            stable_len: 11,
            sealed: false,
            revision: StreamRevision::ZERO,
        },
        LeadingBoundaryState::Pending,
    );

    let first = hosted.prepare_frame(6, 2);
    assert_eq!(first.history.rows.len(), 2);
    assert!(first.history.rows.as_slice()[0].plain_text().is_empty());
    assert!(
        first.history.rows.as_slice()[1]
            .plain_text()
            .contains("hello")
    );
    hosted.apply_commit_success(first.history);
    assert_eq!(hosted.leading_boundary, LeadingBoundaryState::Committed);

    let second = hosted.prepare_frame(6, 1);
    assert!(!second.history.commit_leading_boundary);
    assert!(
        second
            .history
            .rows
            .as_slice()
            .iter()
            .all(|row| !row.plain_text().is_empty())
    );
}

#[test]
fn hosted_assistant_stream_compacts_after_success_and_preserves_atomic_transition() {
    use crate::transcript::model::TranscriptId;
    use crate::transcript::{AssistantStream, SegmentKind};

    let unit_id = TranscriptId(42);
    let mut hosted = HostedStream::new(unit_id, AssistantStream::new(), LeadingBoundaryState::None);

    hosted
        .content_mut()
        .push_delta(SegmentKind::Text, "hello **bo");

    let prepared1 = hosted.prepare_frame(6, 1);
    let plan1 = prepared1.history.semantic_plan.clone();
    assert_eq!(plan1.next_committed_through, StreamOffset::new(6));
    assert_eq!(plan1.payload.rows_to_write(), 1);

    assert_eq!(hosted.committed_through, StreamOffset::ZERO);
    assert_eq!(hosted.snapshot().source_base, StreamOffset::ZERO);

    hosted.apply_commit_success(prepared1.history);

    assert_eq!(hosted.committed_through, StreamOffset::new(6));
    let after_commit = hosted.snapshot();
    assert_eq!(after_commit.source_base, StreamOffset::new(6));

    hosted.content_mut().push_delta(SegmentKind::Text, "ld**");

    let after_append = hosted.snapshot();
    assert!(after_append.validate().is_ok());
    assert_eq!(after_append.source_base, StreamOffset::new(6));
    assert_eq!(after_append.source_end, StreamOffset::new(14));
    assert_eq!(
        after_append.view.nodes[0].owned_range(),
        StreamRange::new(StreamOffset::new(6), StreamOffset::new(14)),
    );

    hosted.seal();
    let prepared2 = hosted.prepare_frame(80, 10);
    let plan2 = prepared2.history.semantic_plan.clone();
    assert_eq!(plan2.next_committed_through, StreamOffset::new(14));

    hosted.apply_commit_success(prepared2.history);
    assert!(hosted.can_handoff());
    assert_eq!(hosted.partial, None);
    assert_eq!(hosted.snapshot().source_base, StreamOffset::new(14));
}

#[test]
fn hosted_stream_lifecycle_survives_sealing() {
    use crate::transcript::model::TranscriptId;

    let unit_id = TranscriptId(42);
    let mock = MockStream {
        source: "line 1\nline 2\nline 3".to_string(),
        stable_len: 14, // covers line 1 (6) + \n (1) + line 2 (6) + \n (1)
        sealed: false,
        revision: StreamRevision::ZERO,
    };

    let mut hosted = HostedStream::new(unit_id, mock, LeadingBoundaryState::None);
    assert_eq!(hosted.committed_through, StreamOffset::ZERO);
    assert!(!hosted.is_sealed());

    // Prepare plan at width 20 requesting 10 rows
    let prepared = hosted.prepare_frame(20, 10);
    let plan = prepared.history.semantic_plan.clone();
    // lines 1 and 2 are stable (2 rows)
    assert_eq!(plan.payload.rows_to_write(), 2);
    assert_eq!(plan.next_committed_through, StreamOffset::new(14));

    // SIMULATE FAILURE: terminal fails to write rows -> apply_commit_success is NOT called!
    assert_eq!(hosted.committed_through, StreamOffset::ZERO);
    assert_eq!(hosted.snapshot().source_base, StreamOffset::ZERO);

    // SIMULATE SUCCESS: apply commit plan
    hosted.apply_commit_success(prepared.history);
    assert_eq!(hosted.committed_through, StreamOffset::new(14));

    // Seal stream in-place: HostedStream retains unit_id, committed_through, and partial state!
    hosted.seal();
    assert!(hosted.is_sealed());
    assert_eq!(hosted.unit_id, unit_id);
    assert_eq!(hosted.committed_through, StreamOffset::new(14));

    // Drain remaining line 3 after sealing
    let prepared2 = hosted.prepare_frame(20, 10);
    let plan2 = prepared2.history.semantic_plan.clone();
    assert_eq!(plan2.payload.rows_to_write(), 1);
    assert_eq!(plan2.next_committed_through, StreamOffset::new(20));
    hosted.apply_commit_success(prepared2.history);

    assert!(hosted.can_handoff());
}

#[test]
fn hosted_stream_seal_before_first_snapshot_can_handoff_before_history_drain() {
    use crate::transcript::model::TranscriptId;

    let unit_id = TranscriptId(42);
    let mock = MockStream {
        source: "abc".into(),
        stable_len: 3,
        sealed: false,
        revision: StreamRevision::ZERO,
    };

    let mut hosted = HostedStream::new(unit_id, mock, LeadingBoundaryState::None);
    // IMPORTANT: never call snapshot() or prepare_frame() before seal
    hosted.seal();
    assert!(hosted.can_handoff());

    let prepared = hosted.prepare_frame(80, 10);
    assert_eq!(prepared.history.semantic_plan.payload.rows_to_write(), 1);
    hosted.apply_commit_success(prepared.history);
    assert!(hosted.can_handoff());
}

#[test]
fn sealed_stream_static_lowering_matches_stream_compilation() {
    let mut stream = crate::transcript::AssistantStream::new();
    stream.push_delta(crate::transcript::SegmentKind::Thinking, "thinking\n");
    stream.push_delta(
        crate::transcript::SegmentKind::Text,
        "# Heading\n- list item\n\nwide 漢 e\u{301}\n**bold**",
    );
    stream.seal();
    let snapshot = stream.snapshot();
    assert!(snapshot.validate().is_ok());

    for width in [6u16, 20, 80] {
        let compiled = compile_stream(&snapshot.view, width, snapshot.source_end);
        let lowered =
            ViewCompiler::default().compile(&snapshot.view.clone().into_static_view(), width);
        let physical = compiled
            .rows
            .iter()
            .map(|row| row.physical.clone())
            .collect::<Vec<_>>();
        assert_rows_equivalent(&physical, &lowered.rows);
    }
}

#[test]
fn atomic_component_identity_is_rejected_at_construction() {
    let component =
        crate::component::ComponentRegistry::new().register(crate::controls::TextInput::new());
    let view = View::component(component);
    let result = std::panic::catch_unwind(|| StreamNode::atomic(range(0, 1), view.into_view()));
    assert!(result.is_err());
}

#[test]
fn nested_component_atomic_is_rejected_by_snapshot_validation() {
    let id = crate::component::ComponentId::allocate();
    let leaf = View::text("x").into_view().with_component(id);
    let view = View::vertical(|column| {
        column.child(
            View::horizontal(|row| {
                row.child(leaf);
            })
            .container(),
        );
    })
    .clamp_rows(1, OverflowIndicator::None);
    let snapshot = StreamSnapshot {
        revision: StreamRevision::ZERO,
        source_base: StreamOffset::ZERO,
        source_end: StreamOffset::new(1),
        stable_through: StreamOffset::new(1),
        view: StreamView::new(vec![StreamNode::Atomic {
            range: range(0, 1),
            view,
        }]),
    };
    assert_eq!(
        snapshot.validate(),
        Err(StreamValidationError::AtomicContainsComponent)
    );
}

#[test]
fn direct_component_atomic_is_rejected_by_snapshot_validation() {
    let id = crate::component::ComponentId::allocate();
    let view = View::text("x").into_view().with_component(id);
    let snapshot = StreamSnapshot {
        revision: StreamRevision::ZERO,
        source_base: StreamOffset::ZERO,
        source_end: StreamOffset::new(1),
        stable_through: StreamOffset::new(1),
        view: StreamView::new(vec![StreamNode::Atomic {
            range: range(0, 1),
            view,
        }]),
    };
    assert_eq!(
        snapshot.validate(),
        Err(StreamValidationError::AtomicContainsComponent)
    );
}
