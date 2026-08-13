use super::*;
use iyon_tui::stream::{StreamOffset, StreamRange, StreamRevision, StreamSnapshotBuilder};
use iyon_tui::{History, TextSpan, Theme};

fn snapshot(stream: &AssistantStream) -> iyon_tui::stream::StreamSnapshot {
    stream.snapshot()
}

#[test]
fn assistant_stream_text_and_thinking_snapshot() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Thinking, "reasoning here");
    stream.push_delta(SegmentKind::Text, "answer text");
    let snap = snapshot(&stream);
    assert_eq!(snap.source_base(), StreamOffset::ZERO);
    assert_eq!(snap.source_end(), StreamOffset::new(27));
}

#[test]
fn markdown_hidden_closer_keeps_a_combining_egc_together() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**a**\u{301} rest");
    stream.seal();
    let snap = snapshot(&stream);
    assert_eq!(snap.stable_through(), snap.source_end());
}

#[test]
fn stability_is_row_relative_after_a_hard_newline() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "done\nopen");
    let snap = snapshot(&stream);
    assert_eq!(snap.source_end(), StreamOffset::new(9));
    assert_eq!(snap.stable_through(), StreamOffset::new(8));
}

#[test]
fn restart_planning_distinguishes_hidden_prefix_and_visible_context() {
    let source = "prefix **bold** suffix";
    let opener = source.find("**").unwrap();
    let closer = source[opener + 2..].find("**").unwrap() + opener + 2;
    let suffix_start = source.find("suffix").unwrap();
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, source);
    let (restart, _) = stream.restart_plan_at(StreamOffset::new(closer as u64));
    assert_eq!(restart, StreamOffset::new(opener as u64));
    stream.compact_before(StreamOffset::new(closer as u64));
    assert_eq!(
        snapshot(&stream).source_base(),
        StreamOffset::new(opener as u64)
    );
    assert!(!format!("{:?}", snapshot(&stream)).contains("**"));
    let suffix_offset = StreamOffset::new((suffix_start + 1) as u64);
    assert_eq!(stream.restart_plan_at(suffix_offset).0, suffix_offset);
    stream.compact_before(suffix_offset);
    assert_eq!(snapshot(&stream).source_base(), suffix_offset);
}

#[test]
fn list_tab_restart_retains_inline_context_after_replacement() {
    let source = "- **before\tinside** tail";
    let opener = source.find("**").unwrap() as u64;
    let inside = source.find("inside").unwrap() as u64;
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, source);
    assert_eq!(
        stream.restart_plan_at(StreamOffset::new(inside + 1)).0,
        StreamOffset::new(opener)
    );
    stream.compact_before(StreamOffset::new(inside + 1));
    let tail_doc = parse_assistant_tail(
        &slice_segments(
            &stream.segments,
            stream.source_base.as_u64() as usize,
            stream.source_end.as_u64() as usize,
        ),
        stream.source_base,
        stream.continuation,
    );
    let inside = tail_doc
        .rows
        .iter()
        .flat_map(|row| row.projected_runs.iter())
        .find(|run| run.display.contains("inside"))
        .expect("tabbed bold suffix");
    assert!(format!("{:?}", inside.style).contains("bold: Some(true)"));
}

#[test]
fn hanging_prefix_that_cannot_fit_blocks_stream_transfer() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "9. nine");
    stream.seal();
    let snap = snapshot(&stream);
    assert_eq!(snap.stable_through(), snap.source_end());
    assert_eq!(snap.source_end(), StreamOffset::new(7));
}

#[test]
fn assistant_stream_markdown_projected_node() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**");
    stream.seal();
    assert_eq!(
        snapshot(&stream).stable_through(),
        snapshot(&stream).source_end()
    );
}

#[test]
fn hard_newline_source_ownership_simple() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "abc\ndef");
    stream.seal();
    let snap = snapshot(&stream);
    assert_eq!(snap.source_end(), StreamOffset::new(7));
    assert_eq!(snap.stable_through(), snap.source_end());
}

#[test]
fn hard_newline_source_ownership_consecutive() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "a\n\nb");
    stream.seal();
    let snap = snapshot(&stream);
    assert_eq!(snap.source_end(), StreamOffset::new(4));
    assert_eq!(snap.stable_through(), snap.source_end());
}

#[test]
fn hard_newline_source_ownership_trailing() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "a\n");
    let snap = snapshot(&stream);
    assert_eq!(snap.source_end(), StreamOffset::new(2));
    assert_eq!(snap.stable_through(), snap.source_end());
}

#[test]
fn hard_newline_source_ownership_wrapped() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "abcdefgh\nx");
    stream.seal();
    let snap = snapshot(&stream);
    assert_eq!(snap.source_end(), StreamOffset::new(10));
    assert_eq!(snap.stable_through(), snap.source_end());
}

#[test]
fn hard_newline_source_ownership_projected_markdown() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**\nplain");
    stream.seal();
    let snap = snapshot(&stream);
    assert_eq!(snap.source_end(), StreamOffset::new(14));
    assert_eq!(snap.stable_through(), snap.source_end());
}

#[test]
fn atomic_completed_line_is_stable_when_open() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**\n");
    let snap = snapshot(&stream);
    assert_eq!(snap.stable_through(), StreamOffset::new(9));
    assert_eq!(snap.source_end(), StreamOffset::new(9));
}

#[test]
fn atomic_closed_bold_touching_eof_is_unstable_when_open() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::ZERO);
}

#[test]
fn atomic_closed_bold_pinned_by_following_source_is_stable_only_before_open_egc() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold** x");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::new(9));
}

#[test]
fn trailing_unclosed_bold_stops_stability() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**unclosed");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::ZERO);
}

#[test]
fn trailing_list_with_unfinished_markdown_is_unstable() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "- item **bo");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::new(7));
}

#[test]
fn trailing_list_with_ordinary_body_waits_for_egc_safety() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "- item");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::new(5));
}

#[test]
fn ordinary_list_becomes_committable_after_newline() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "- item\n");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::new(7));
}

#[test]
fn whitespace_only_open_prefix_cannot_commit_before_classification() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "  ");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::ZERO);
    for suffix in ["- item", "# heading", "1. ordered"] {
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "  ");
        assert_eq!(snapshot(&stream).stable_through(), StreamOffset::ZERO);
        stream.push_delta(SegmentKind::Text, suffix);
        assert_eq!(snapshot(&stream).source_base(), StreamOffset::ZERO);
        let row = &parse_assistant(&stream.segments).rows[0];
        match suffix {
            "# heading" => assert!(matches!(row.layout, AssistantRowLayout::Heading)),
            "- item" | "1. ordered" => {
                assert!(matches!(row.layout, AssistantRowLayout::ListItem { .. }))
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn mixed_atomic_exact_mutable_tail_stability() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "**bold**\nplain\nmutable");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::new(21));
}

#[test]
fn compact_before_preserves_atomic_transition_without_duplicating_committed_source() {
    let mut stream = AssistantStream::new();
    stream.push_delta(SegmentKind::Text, "hello **bo");
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::new(6));
    stream.compact_before(StreamOffset::new(6));
    stream.push_delta(SegmentKind::Text, "ld**");
    let snap = snapshot(&stream);
    assert_eq!(snap.source_base(), StreamOffset::new(6));
    assert_eq!(snap.source_end(), StreamOffset::new(14));
    assert_eq!(
        slice_segments(&stream.segments, 6, 14),
        [AssistantSegment::Text("**bold**".into())]
    );
    stream.seal();
    assert_eq!(snapshot(&stream).stable_through(), StreamOffset::new(14));
}

#[test]
fn wrapped_heading_continuation_retains_heading_styling_after_compaction() {
    let mut stream = AssistantStream::new();
    stream.push_delta(
        SegmentKind::Text,
        "## long heading that wraps across multiple physical rows",
    );
    stream.compact_before(StreamOffset::new(10));
    assert_eq!(snapshot(&stream).source_base(), StreamOffset::new(10));
    let tail_doc = parse_assistant_tail(
        &slice_segments(&stream.segments, 10, stream.source_end.as_u64() as usize),
        StreamOffset::new(10),
        stream.continuation,
    );
    assert_eq!(tail_doc.rows[0].layout, AssistantRowLayout::Heading);
}

#[test]
#[should_panic]
fn sealed_assistant_stream_rejects_append() {
    let mut stream = AssistantStream::new();
    stream.seal();
    stream.push_delta(SegmentKind::Text, "late");
}

#[test]
fn paragraph_continuation_classification_retains_paragraph_after_compaction() {
    for suffix in ["- item", "# heading", "1. ordered"] {
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, &format!("prefix {suffix}"));
        stream.compact_before(StreamOffset::new(7));
        let snap = snapshot(&stream);
        assert_eq!(snap.source_base(), StreamOffset::new(7));
        let tail_doc = parse_assistant_tail(
            &slice_segments(&stream.segments, 7, stream.source_end.as_u64() as usize),
            StreamOffset::new(7),
            stream.continuation,
        );
        assert_eq!(tail_doc.rows[0].layout, AssistantRowLayout::Plain);
    }
}

#[test]
fn public_snapshot_builder_remains_usable_for_external_streams() {
    let snapshot = StreamSnapshotBuilder::new(
        StreamRevision::new(1),
        StreamOffset::ZERO,
        StreamOffset::new(1),
        StreamOffset::new(1),
    )
    .exact_text(
        StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
        [TextSpan::plain("x")],
    )
    .finish()
    .unwrap();
    assert_eq!(snapshot.source_end(), StreamOffset::new(1));
    let _ = History::new();
    let _ = Theme::default();
}
