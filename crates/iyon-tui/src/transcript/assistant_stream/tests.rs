use super::*;
use crate::{
    stream::plan_stream_transfer,
    stream::{StreamProvenance, StreamRowTransfer, compile_stream, projected_atoms},
    transcript::hosted_stream::{HostedStream, LeadingBoundaryState},
};

#[test]
fn assistant_stream_text_and_thinking_snapshot() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Thinking, "reasoning here");
    stream.push_delta(SegmentKind::Text, "answer text");

    let snap = stream.snapshot();
    assert!(snap.validate().is_ok());
    assert_eq!(snap.source_base, StreamOffset::ZERO);
    assert_eq!(snap.source_end, StreamOffset::new(27)); // "reasoning here\n\nanswer text"
}

#[test]
fn markdown_hidden_closer_keeps_a_combining_egc_together() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**a**\u{301} rest");
    stream.seal();
    let snapshot = stream.snapshot();
    assert!(snapshot.validate().is_ok());
    let projected = snapshot
        .view
        .nodes
        .iter()
        .find_map(|node| match node {
            StreamNode::Text(text) => Some(text),
            StreamNode::Atomic { .. } => None,
        })
        .expect("assistant Markdown should produce projected text");
    assert!(
        projected_atoms(projected)
            .iter()
            .any(|atom| atom.display == "a\u{301}")
    );
}

#[test]
fn stability_is_row_relative_after_a_hard_newline() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "done\nopen");

    let snapshot = stream.snapshot();
    assert_eq!(snapshot.source_end, StreamOffset::new(9));
    assert_eq!(snapshot.stable_through, StreamOffset::new(8));
}

#[test]
fn projected_markdown_paragraph_spills_before_seal() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(1),
        AssistantStream::new(),
        LeadingBoundaryState::None,
    );
    let chunks = [
        "ordinary **bold",
        " words** ordinary ordinary ",
        "ordinary ordinary ordinary ordinary ordinary",
    ];
    let mut committed = Vec::new();
    for chunk in chunks {
        hosted.content_mut().push_delta(SegmentKind::Text, chunk);
        let prepared = hosted.prepare_frame(12, 1);
        if !prepared.history.rows.is_empty() {
            hosted.apply_commit_success(prepared.history);
        }
        committed.push(hosted.committed_through);
    }
    assert!(committed[1] > committed[0]);
    assert!(committed[2] > committed[1]);
    let snapshot = hosted.snapshot();
    assert!(snapshot.source_base <= hosted.committed_through);
    assert!(snapshot.validate().is_ok());
}

#[test]
fn projected_compaction_retains_inline_restart_context() {
    let source = "prefix **abcdefghijklmnop long bold text** suffix";
    let opener = source.find("**").unwrap() as u64;
    let closer = source[opener as usize + 2..].find("**").unwrap() as u64 + opener + 2;
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(2),
        AssistantStream::new(),
        LeadingBoundaryState::None,
    );
    hosted.content_mut().push_delta(SegmentKind::Text, source);

    let mut committed_inside = false;
    for _ in 0..32 {
        let prepared = hosted.prepare_frame(6, 1);
        if prepared.history.rows.is_empty() {
            break;
        }
        hosted.apply_commit_success(prepared.history);
        if hosted.committed_through.as_u64() > opener + 2
            && hosted.committed_through.as_u64() < closer
        {
            committed_inside = true;
            let snapshot = hosted.snapshot();
            assert_eq!(snapshot.source_base, StreamOffset::new(opener));
            let suffix = snapshot.view.suffix_from(hosted.committed_through);
            let rendered = suffix.into_static_view();
            let text = format!("{rendered:?}");
            assert!(!text.contains("**"));
        }
    }
    assert!(committed_inside);

    hosted.content_mut().push_delta(SegmentKind::Text, "\n");
    for _ in 0..64 {
        let prepared = hosted.prepare_frame(8, 1);
        if prepared.history.rows.is_empty() {
            break;
        }
        hosted.apply_commit_success(prepared.history);
    }
    let final_snapshot = hosted.snapshot();
    assert_eq!(final_snapshot.source_base, final_snapshot.source_end);
}

#[test]
fn restart_planning_distinguishes_hidden_prefix_and_visible_context() {
    let source = "prefix **bold** suffix";
    let opener = source.find("**").unwrap();
    let closer = source[opener + 2..].find("**").unwrap() + opener + 2;
    let suffix_start = source.find("suffix").unwrap();
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, source);

    let (restart_before_closer, _) = stream.restart_plan_at(StreamOffset::new(closer as u64));
    assert_eq!(restart_before_closer, StreamOffset::new(opener as u64));

    stream.compact_before(StreamOffset::new(closer as u64));
    let snapshot = stream.snapshot();
    assert_eq!(snapshot.source_base, StreamOffset::new(opener as u64));
    assert!(!format!("{snapshot:?}").contains("**"));

    let suffix_offset = StreamOffset::new((suffix_start + 1) as u64);
    let (restart_in_suffix, _) = stream.restart_plan_at(suffix_offset);
    assert_eq!(restart_in_suffix, suffix_offset);
    stream.compact_before(suffix_offset);
    assert_eq!(stream.snapshot().source_base, suffix_offset);
}

#[test]
fn hosted_nested_compaction_preserves_outer_and_inner_styles() {
    let source = "prefix **outer _inner content which wraps_ outer** suffix";
    let opener = source.find("**").unwrap() as u64;
    let inner_start = source.find("inner").unwrap() as u64;
    let outer_close = source[opener as usize + 2..].find("**").unwrap() as u64 + opener + 2;
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(4),
        AssistantStream::new(),
        LeadingBoundaryState::None,
    );
    hosted.content_mut().push_delta(SegmentKind::Text, source);

    let mut committed_inside = false;
    for _ in 0..64 {
        let prepared = hosted.prepare_frame(8, 1);
        if prepared.history.rows.is_empty() {
            break;
        }
        hosted.apply_commit_success(prepared.history);
        let committed = hosted.committed_through.as_u64();
        if committed > inner_start && committed < outer_close {
            committed_inside = true;
            let snapshot = hosted.snapshot();
            assert_eq!(snapshot.source_base, StreamOffset::new(opener));
            let inner = snapshot
                .view
                .nodes
                .iter()
                .find_map(|node| match node {
                    StreamNode::Text(text) => {
                        text.runs.iter().find(|run| run.display.contains("inner"))
                    }
                    StreamNode::Atomic { .. } => None,
                })
                .expect("nested live suffix");
            assert_eq!(inner.style.attributes.bold, Some(true));
            assert_eq!(inner.style.attributes.italic, Some(true));
            break;
        }
    }
    assert!(committed_inside);
}

#[test]
fn list_tab_restart_retains_inline_context_after_replacement() {
    let source = "- **before\tinside** tail";
    let opener = source.find("**").unwrap() as u64;
    let inside = source.find("inside").unwrap() as u64;
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, source);

    let (restart, _) = stream.restart_plan_at(StreamOffset::new(inside + 1));
    assert_eq!(restart, StreamOffset::new(opener));

    stream.compact_before(StreamOffset::new(inside + 1));
    let snapshot = stream.snapshot();
    let inside = snapshot
        .view
        .nodes
        .iter()
        .find_map(|node| match node {
            StreamNode::Text(text) => text.runs.iter().find(|run| run.display.contains("inside")),
            StreamNode::Atomic { .. } => None,
        })
        .expect("tabbed bold suffix");
    assert_eq!(inside.style.attributes.bold, Some(true));
}

#[test]
fn hanging_prefix_that_cannot_fit_blocks_stream_transfer() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "9. nine");
    stream.seal();
    let snapshot = stream.snapshot();
    let compiled = compile_stream(&snapshot.view, 3, snapshot.source_end);

    assert!(!compiled.rows.is_empty());
    assert_eq!(compiled.transferable_prefix_rows, 0);
}

#[test]
fn projected_list_item_spills_and_retains_hanging_context() {
    let mut hosted = HostedStream::new(
        crate::transcript::model::TranscriptId(3),
        AssistantStream::new(),
        LeadingBoundaryState::None,
    );
    hosted.content_mut().push_delta(
        SegmentKind::Text,
        "- **a very long list item that keeps wrapping** and continues",
    );
    let mut committed = false;
    for _ in 0..32 {
        let prepared = hosted.prepare_frame(8, 1);
        if prepared.history.rows.is_empty() {
            break;
        }
        hosted.apply_commit_success(prepared.history);
        committed |= hosted.committed_through > StreamOffset::new(2);
    }
    assert!(committed);
    assert!(hosted.snapshot().validate().is_ok());
}

#[test]
fn assistant_stream_markdown_projected_node() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold text**\nplain text");

    let snap = stream.snapshot();
    assert_eq!(snap.view.nodes.len(), 2);
    // Markdown styling is projected text, not a row-wide atomic group.
    assert!(matches!(
        snap.view.nodes[0].provenance(),
        StreamProvenance::Projected(_)
    ));
    assert!(matches!(
        snap.view.nodes[1].provenance(),
        StreamProvenance::Projected(_)
    ));
}

#[test]
fn hard_newline_source_ownership_simple() {
    // Test A: "abc\ndef" -> row "abc" commits through 4, row "def" commits through 7
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "abc\ndef");
    stream.seal();

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(7));
    assert_eq!(snap.view.nodes.len(), 2);

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.rows.len(), 2);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(4))
    );
    assert_eq!(
        compiled.rows[1].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(7))
    );
}

#[test]
fn hard_newline_source_ownership_consecutive() {
    // Test B: "a\n\nb" -> physical rows: a, blank, b -> boundaries: 2, 3, 4
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "a\n\nb");
    stream.seal();

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(4));
    assert_eq!(snap.view.nodes.len(), 3);

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.rows.len(), 3);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(2))
    );
    assert_eq!(
        compiled.rows[1].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(3))
    );
    assert_eq!(
        compiled.rows[2].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(4))
    );
}

#[test]
fn hard_newline_source_ownership_trailing() {
    // Test C: "a\n" -> source accounting reaches exactly source_end = 2
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "a\n");
    stream.seal();

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(2));
    assert_eq!(snap.view.nodes.len(), 1);

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.rows.len(), 1);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(2))
    );
}

#[test]
fn hard_newline_source_ownership_wrapped() {
    // Test D: "abcdefgh\nx" at width = 4 -> abcd -> 4, efgh -> 9 (owns \n), x -> 10
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "abcdefgh\nx");
    stream.seal();

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(10));
    assert_eq!(snap.view.nodes.len(), 2);

    let compiled = compile_stream(&snap.view, 4, snap.stable_through);
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
fn hard_newline_source_ownership_projected_markdown() {
    // Test E: "**bold**\nplain" -> projected text owns through newline (9).
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**\nplain");
    stream.seal();

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(14));
    assert_eq!(snap.view.nodes.len(), 2);

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.rows.len(), 2);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(9))
    );
    assert_eq!(
        compiled.rows[1].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(14))
    );
}

// --- Fix 1: stability independent of provenance ---

#[test]
fn atomic_completed_line_is_stable_when_open() {
    // A projected Markdown row with a hard newline is stable.
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**\n");
    // NOT sealed

    let snap = stream.snapshot();
    assert!(snap.validate().is_ok());
    assert_eq!(snap.source_end, StreamOffset::new(9));
    // Projected text with newline should be stable through source_end.
    assert_eq!(snap.stable_through, StreamOffset::new(9));

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.transferable_prefix_rows, 1);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(9))
    );
}

#[test]
fn atomic_closed_bold_touching_eof_is_unstable_when_open() {
    // A closed bold span touching EOF remains semantically unstable.
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**");
    // NOT sealed

    let snap = stream.snapshot();
    assert!(snap.validate().is_ok());
    assert_eq!(snap.source_end, StreamOffset::new(8));
    // Closed bold touching EOF is unstable because closing run could grow
    assert_eq!(snap.stable_through, StreamOffset::ZERO);

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.transferable_prefix_rows, 0);
    assert!(matches!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Blocked
    ));
}

#[test]
fn atomic_closed_bold_pinned_by_following_source_is_stable_only_before_open_egc() {
    // "**bold** x" is semantically pinned, but the final `x` EGC can still
    // be extended by a future append.
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold** x");
    // NOT sealed

    let snap = stream.snapshot();
    assert!(snap.validate().is_ok());
    assert_eq!(snap.source_end, StreamOffset::new(10));
    assert_eq!(snap.stable_through, StreamOffset::new(9));

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.transferable_prefix_rows, 0);
    assert!(matches!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Blocked
    ));
}

#[test]
fn trailing_unclosed_bold_stops_stability() {
    // "**bold" without closing delimiter => stable_prefix_len stops before "**"
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold");
    // NOT sealed

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(6));
    // stable_through should NOT advance into the unclosed bold opener
    assert_eq!(snap.stable_through, StreamOffset::ZERO);
}

#[test]
fn trailing_list_with_unfinished_markdown_is_unstable() {
    // "- item **bo" stops semantic stability at byte 7.
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "- item **bo");
    // NOT sealed

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(11));
    assert_eq!(snap.stable_through, StreamOffset::new(7));

    // The projected list body cannot commit across the unstable delimiter.
    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.transferable_prefix_rows, 0);
    assert!(matches!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Blocked
    ));
}

#[test]
fn trailing_list_with_ordinary_body_waits_for_egc_safety() {
    // "- item" is semantically determined, but the final `m` EGC remains
    // open. The final physical row therefore cannot commit until a newline/seal.
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "- item");
    // NOT sealed

    let snap = stream.snapshot();
    assert_eq!(snap.source_end, StreamOffset::new(6));
    assert_eq!(snap.stable_through, StreamOffset::new(5));

    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.transferable_prefix_rows, 0);
    assert!(matches!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Blocked
    ));
}

#[test]
fn ordinary_list_becomes_committable_after_newline() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "- item\n");

    let snap = stream.snapshot();
    assert_eq!(snap.stable_through, StreamOffset::new(7));
    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert_eq!(compiled.transferable_prefix_rows, 1);
    assert_eq!(
        compiled.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(7))
    );
}

#[test]
fn whitespace_only_open_prefix_cannot_commit_before_classification() {
    for suffix in ["- item", "# heading", "1. ordered"] {
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "  ");

        let phase_one = stream.snapshot();
        assert_eq!(phase_one.source_base, StreamOffset::ZERO);
        assert_eq!(phase_one.stable_through, StreamOffset::ZERO);
        let compiled_one = compile_stream(&phase_one.view, 80, phase_one.stable_through);
        assert_eq!(
            compiled_one.transferable_prefix_rows, 0,
            "suffix={suffix:?}"
        );
        let plan = plan_stream_transfer(&compiled_one, 10, None, StreamOffset::ZERO);
        assert_eq!(plan.payload.rows_to_write(), 0);
        assert_eq!(plan.next_committed_through, StreamOffset::ZERO);

        stream.push_delta(SegmentKind::Text, suffix);
        let phase_two = stream.snapshot();
        assert!(phase_two.validate().is_ok());
        assert_eq!(phase_two.source_base, StreamOffset::ZERO);
        assert_eq!(
            phase_two.view.nodes[0].owned_range().start,
            StreamOffset::ZERO
        );
        assert_eq!(phase_two.view.nodes.len(), 1, "suffix={suffix:?}");
        assert!(matches!(
            phase_two.view.nodes[0].provenance(),
            StreamProvenance::Projected(_)
        ));
    }
}

#[test]
fn mixed_atomic_exact_mutable_tail_stability() {
    // "**bold**\nplain\nmutable"
    // "**bold**\n" = 9 bytes, "plain\n" = 6 bytes, "mutable" = 7 bytes => total = 22
    // Projected completed line (0..9) => stable
    // Projected completed line "plain\n" (9..15) => stable
    // Exact mutable tail "mutable" (15..22) => stability stops before trailing EGC
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**\nplain\nmutable");
    // NOT sealed

    let snap = stream.snapshot();
    assert!(snap.validate().is_ok());
    assert_eq!(snap.source_end, StreamOffset::new(22));

    // stable_through must reach through both completed rows but stop before trailing EGC
    // append_only_text_stable_frontier("mutable", 15, false)
    // "mutable" graphemes: m(0) u(1) t(2) a(3) b(4) l(5) e(6) - last offset = 6
    // frontier = 15 + 6 = 21
    assert!(snap.stable_through >= StreamOffset::new(15));
    assert!(snap.stable_through < snap.source_end);

    // The completed projected lines must be committable.
    let compiled = compile_stream(&snap.view, 80, snap.stable_through);
    assert!(compiled.transferable_prefix_rows >= 2);
}

// --- Compaction & continuation regressions ---

#[test]
fn compact_before_preserves_atomic_transition_without_duplicating_committed_source() {
    // Phase 1: push "hello **bo"
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "hello **bo");

    let snap1 = stream.snapshot();
    assert!(snap1.validate().is_ok());
    assert_eq!(snap1.source_end, StreamOffset::new(10));
    assert_eq!(snap1.stable_through, StreamOffset::new(6)); // "hello "

    // Compile stream at width 6: "hello " wraps to row 0 and is committable prefix
    let compiled1 = compile_stream(&snap1.view, 6, snap1.stable_through);
    assert_eq!(compiled1.transferable_prefix_rows, 1);
    assert_eq!(
        compiled1.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(6))
    );

    // Apply compaction at 6 (as HostedStream does upon successful commit)
    stream.compact_before(StreamOffset::new(6));

    // Phase 2: push "ld**"
    stream.push_delta(SegmentKind::Text, "ld**");

    let snap2 = stream.snapshot();
    assert!(snap2.validate().is_ok());
    assert_eq!(snap2.source_base, StreamOffset::new(6));
    assert_eq!(snap2.source_end, StreamOffset::new(14));

    // The retained suffix starts at the acknowledged source checkpoint.
    assert_eq!(snap2.view.nodes.len(), 1);
    assert_eq!(
        snap2.view.nodes[0].owned_range(),
        StreamRange::new(StreamOffset::new(6), StreamOffset::new(14))
    );

    // Seal and plan remainder
    stream.seal();
    let snap3 = stream.snapshot();
    assert!(snap3.validate().is_ok());
    assert_eq!(snap3.stable_through, StreamOffset::new(14));

    let compiled2 = compile_stream(&snap3.view, 80, snap3.stable_through);
    assert_eq!(compiled2.transferable_prefix_rows, 1);
    assert_eq!(
        compiled2.rows[0].transfer,
        StreamRowTransfer::Checkpoint(StreamOffset::new(14))
    );
}

#[test]
fn wrapped_heading_continuation_retains_heading_styling_after_compaction() {
    let mut stream = AssistantStream::new();
    stream.push_delta(
        SegmentKind::Text,
        "## long heading that wraps across multiple physical rows",
    );

    let snap1 = stream.snapshot();
    assert!(snap1.validate().is_ok());
    assert_eq!(snap1.view.nodes.len(), 1);

    // Compact before byte 10 (inside the heading)
    stream.compact_before(StreamOffset::new(10));

    let snap2 = stream.snapshot();
    assert!(snap2.validate().is_ok());
    assert_eq!(snap2.source_base, StreamOffset::new(10));
    assert_eq!(snap2.view.nodes.len(), 1);

    // Tail node must retain heading layout/styling
    let tail_doc = parse_assistant_tail(
        &slice_segments(&stream.segments, 10, snap2.source_end.as_u64() as usize),
        StreamOffset::new(10),
        stream.continuation,
    );
    assert_eq!(tail_doc.rows.len(), 1);
    assert_eq!(tail_doc.rows[0].layout, AssistantRowLayout::Heading);
}

#[test]
#[should_panic(expected = "cannot append to sealed AssistantStream")]
fn sealed_assistant_stream_rejects_append() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "old");
    stream.seal();
    stream.push_delta(SegmentKind::Text, "new");
}

#[test]
fn paragraph_continuation_classification_retains_paragraph_after_compaction() {
    // 1. "prefix - item"
    let mut stream1 = AssistantStream::new();
    stream1.push_delta(SegmentKind::Text, "prefix - item");
    stream1.compact_before(StreamOffset::new(7)); // "prefix "
    let snap1 = stream1.snapshot();
    assert!(snap1.validate().is_ok());
    assert_eq!(snap1.source_base, StreamOffset::new(7));
    let tail_doc1 = parse_assistant_tail(
        &slice_segments(&stream1.segments, 7, snap1.source_end.as_u64() as usize),
        StreamOffset::new(7),
        stream1.continuation,
    );
    assert_eq!(tail_doc1.rows[0].layout, AssistantRowLayout::Plain);

    // 2. "prefix # heading"
    let mut stream2 = AssistantStream::new();
    stream2.push_delta(SegmentKind::Text, "prefix # heading");
    stream2.compact_before(StreamOffset::new(7)); // "prefix "
    let snap2 = stream2.snapshot();
    assert!(snap2.validate().is_ok());
    assert_eq!(snap2.source_base, StreamOffset::new(7));
    let tail_doc2 = parse_assistant_tail(
        &slice_segments(&stream2.segments, 7, snap2.source_end.as_u64() as usize),
        StreamOffset::new(7),
        stream2.continuation,
    );
    assert_eq!(tail_doc2.rows[0].layout, AssistantRowLayout::Plain);

    // 3. "prefix 1. ordered"
    let mut stream3 = AssistantStream::new();
    stream3.push_delta(SegmentKind::Text, "prefix 1. ordered");
    stream3.compact_before(StreamOffset::new(7)); // "prefix "
    let snap3 = stream3.snapshot();
    assert!(snap3.validate().is_ok());
    assert_eq!(snap3.source_base, StreamOffset::new(7));
    let tail_doc3 = parse_assistant_tail(
        &slice_segments(&stream3.segments, 7, snap3.source_end.as_u64() as usize),
        StreamOffset::new(7),
        stream3.continuation,
    );
    assert_eq!(tail_doc3.rows[0].layout, AssistantRowLayout::Plain);
}
