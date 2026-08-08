//! Assistant-specific streaming adapter implementing [`StreamingContent`].
//!
//! Encapsulates Thinking styling, Markdown interpretation, thinking -> text
//! newline separation, and the width-independent stability frontier.

use crate::presentation::{
    StreamNode, StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamView,
    StreamingContent,
};
use crate::transcript::markdown::{
    AssistantContinuation, AssistantDocument, AssistantRowLayout, assistant_row_view,
    parse_assistant, parse_assistant_tail,
};
use crate::transcript::model::{
    AssistantSegment, SegmentKind, slice_segments, think_to_text_newline,
};

#[cfg(test)]
use crate::presentation::stream::{
    StreamAtomicId, StreamProvenance, StreamRowCommit, compile_stream, plan_commit,
};

#[derive(Debug)]
pub(crate) struct AssistantStream {
    segments: Vec<AssistantSegment>,
    source_base: StreamOffset,
    source_end: StreamOffset,
    continuation: Option<AssistantContinuation>,
    revision: StreamRevision,
    sealed: bool,
}

impl AssistantStream {
    pub(crate) fn new() -> Self {
        Self {
            segments: Vec::new(),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::ZERO,
            continuation: None,
            revision: StreamRevision::ZERO,
            sealed: false,
        }
    }

    pub(crate) fn push_delta(&mut self, kind: SegmentKind, chunk: &str) {
        assert!(!self.sealed, "cannot append to sealed AssistantStream");
        if chunk.is_empty() {
            return;
        }

        let chunk = think_to_text_newline(&self.segments, kind, chunk);
        let chunk_len = chunk.len() as u64;

        match kind {
            SegmentKind::Text => {
                if let Some(AssistantSegment::Text(text)) = self.segments.last_mut() {
                    text.push_str(&chunk);
                } else {
                    self.segments
                        .push(AssistantSegment::Text(chunk.into_owned()));
                }
            }
            SegmentKind::Thinking => {
                if let Some(AssistantSegment::Thinking(text)) = self.segments.last_mut() {
                    text.push_str(&chunk);
                } else {
                    self.segments
                        .push(AssistantSegment::Thinking(chunk.into_owned()));
                }
            }
        }

        self.source_end = self.source_end.saturating_add(chunk_len);
        self.revision = self.revision.next();
    }

    pub(crate) fn segments(&self) -> &[AssistantSegment] {
        &self.segments
    }

    pub(crate) fn is_sealed(&self) -> bool {
        self.sealed
    }

    fn continuation_at(&self, offset: StreamOffset) -> Option<AssistantContinuation> {
        let doc = parse_assistant(&self.segments);
        let mut cursor = 0usize;
        let target = offset.as_u64() as usize;

        for row in &doc.rows {
            let row_total_len = row.source.total_len();
            let row_end = cursor + row_total_len;

            if target <= cursor {
                return None;
            }

            if target < row_end {
                debug_assert!(
                    !row.source.restricted,
                    "Atomic/restricted row must not have partial physical cursor advancement: target={}, cursor={}, row_end={}",
                    target, cursor, row_end
                );
                return match row.layout {
                    AssistantRowLayout::Heading => Some(AssistantContinuation::Heading),
                    AssistantRowLayout::Plain => Some(AssistantContinuation::Paragraph),
                    AssistantRowLayout::ListItem { .. } => {
                        panic!(
                            "ListItem must be restricted and cannot have partial cursor advancement"
                        );
                    }
                };
            }

            if target == row_end {
                return None;
            }

            cursor = row_end;
        }

        None
    }
}

impl StreamingContent for AssistantStream {
    fn snapshot(&self) -> StreamSnapshot {
        let tail_segments = slice_segments(
            &self.segments,
            self.source_base.as_u64() as usize,
            self.source_end.as_u64() as usize,
        );
        let doc = parse_assistant_tail(&tail_segments, self.source_base, self.continuation);
        let (view, stable_through) =
            build_assistant_stream_view(&doc, self.source_base, self.source_end, self.sealed);

        StreamSnapshot {
            revision: self.revision,
            source_base: self.source_base,
            source_end: self.source_end,
            stable_through,
            view,
        }
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        if offset <= self.source_base {
            return;
        }

        debug_assert!(offset <= self.source_end);

        self.continuation = self.continuation_at(offset);
        self.source_base = offset;
        self.revision = self.revision.next();
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

/// Builds the [`StreamView`] and computes the width-independent semantic stability frontier.
fn build_assistant_stream_view(
    doc: &AssistantDocument,
    source_base: StreamOffset,
    source_end: StreamOffset,
    sealed: bool,
) -> (StreamView, StreamOffset) {
    let mut nodes = Vec::new();
    let mut cursor = source_base;
    let mut stable_through = source_base;

    for row in &doc.rows {
        let owned_end;

        if row.source.restricted {
            let total_len = row.source.total_len() as u64;
            let range = StreamRange::new(cursor, cursor.saturating_add(total_len));
            owned_end = range.end;
            nodes.push(StreamNode::atomic(range, assistant_row_view(row)));
        } else {
            let text_end = cursor.saturating_add(row.source.content_len as u64);
            let text_range = StreamRange::new(cursor, text_end);
            let node =
                StreamNode::exact_line(text_range, row.spans.clone(), row.source.has_newline);
            owned_end = node.owned_range().end;
            nodes.push(node);
        }

        // The parser's stable_prefix_len is already the truthful append-safe
        // source frontier: semantic stability intersected with source EGC
        // append safety. Provenance only controls commit granularity below.
        if sealed || row.source.has_newline {
            stable_through = owned_end;
        } else {
            stable_through = cursor.saturating_add(row.source.stable_prefix_len as u64);
        }

        cursor = owned_end;
    }

    if sealed || nodes.is_empty() {
        stable_through = source_end;
    }

    (StreamView::new(nodes), stable_through.min(source_end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_stream_text_and_thinking_snapshot() {
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Thinking, "reasoning here");
        stream.push_delta(SegmentKind::Text, "answer text");

        let snap = stream.snapshot();
        assert!(snap.validate());
        assert_eq!(snap.source_base, StreamOffset::ZERO);
        assert_eq!(snap.source_end, StreamOffset::new(27)); // "reasoning here\n\nanswer text"
    }

    #[test]
    fn assistant_stream_markdown_atomic_node() {
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "**bold text**\nplain text");

        let snap = stream.snapshot();
        assert_eq!(snap.view.nodes.len(), 2);
        // First node (**bold text**) is atomic
        assert!(matches!(
            snap.view.nodes[0].provenance(),
            StreamProvenance::Atomic(_)
        ));
        // Second node is exact
        assert!(matches!(
            snap.view.nodes[1].provenance(),
            StreamProvenance::Exact(_)
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
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(4))
        );
        assert_eq!(
            compiled.commit[1],
            StreamRowCommit::Exact(StreamOffset::new(7))
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
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(2))
        );
        assert_eq!(
            compiled.commit[1],
            StreamRowCommit::Exact(StreamOffset::new(3))
        );
        assert_eq!(
            compiled.commit[2],
            StreamRowCommit::Exact(StreamOffset::new(4))
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
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(2))
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
            compiled.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(4))
        );
        assert_eq!(
            compiled.commit[1],
            StreamRowCommit::Exact(StreamOffset::new(9))
        );
        assert_eq!(
            compiled.commit[2],
            StreamRowCommit::Exact(StreamOffset::new(10))
        );
    }

    #[test]
    fn hard_newline_source_ownership_atomic_markdown() {
        // Test E: "**bold**\nplain" -> Atomic owns through newline (9) before plain begins
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "**bold**\nplain");
        stream.seal();

        let snap = stream.snapshot();
        assert_eq!(snap.source_end, StreamOffset::new(14));
        assert_eq!(snap.view.nodes.len(), 2);

        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert_eq!(compiled.rows.len(), 2);
        assert_eq!(
            compiled.commit[0],
            StreamRowCommit::Atomic {
                group: StreamAtomicId(1),
                source_end: StreamOffset::new(9)
            }
        );
        assert_eq!(
            compiled.commit[1],
            StreamRowCommit::Exact(StreamOffset::new(14))
        );
    }

    // --- Fix 1: stability independent of provenance ---

    #[test]
    fn atomic_completed_line_is_stable_when_open() {
        // "**bold**\n" is restricted + has_newline => Atomic AND stable
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "**bold**\n");
        // NOT sealed

        let snap = stream.snapshot();
        assert!(snap.validate());
        assert_eq!(snap.source_end, StreamOffset::new(9));
        // Atomic with newline should be stable through source_end
        assert_eq!(snap.stable_through, StreamOffset::new(9));

        // Atomic group should be eligible to spill
        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert_eq!(compiled.committable_prefix_rows, 1);
        assert!(matches!(compiled.commit[0], StreamRowCommit::Atomic { .. }));
    }

    #[test]
    fn atomic_closed_bold_touching_eof_is_unstable_when_open() {
        // "**bold**" (closed bold touching EOF) => restricted, stable_prefix_len == 0
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "**bold**");
        // NOT sealed

        let snap = stream.snapshot();
        assert!(snap.validate());
        assert_eq!(snap.source_end, StreamOffset::new(8));
        // Closed bold touching EOF is unstable because closing run could grow
        assert_eq!(snap.stable_through, StreamOffset::ZERO);

        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert_eq!(compiled.committable_prefix_rows, 0);
        assert!(matches!(compiled.commit[0], StreamRowCommit::Blocked));
    }

    #[test]
    fn atomic_closed_bold_pinned_by_following_source_is_stable_only_before_open_egc() {
        // "**bold** x" is semantically pinned, but the final `x` EGC can still
        // be extended by a future append.
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "**bold** x");
        // NOT sealed

        let snap = stream.snapshot();
        assert!(snap.validate());
        assert_eq!(snap.source_end, StreamOffset::new(10));
        assert_eq!(snap.stable_through, StreamOffset::new(9));

        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert_eq!(compiled.committable_prefix_rows, 0);
        assert!(matches!(compiled.commit[0], StreamRowCommit::Blocked));
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
        // "- item **bo" => restricted = true, stable_prefix_len = 7 < 11
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "- item **bo");
        // NOT sealed

        let snap = stream.snapshot();
        assert_eq!(snap.source_end, StreamOffset::new(11));
        assert_eq!(snap.stable_through, StreamOffset::new(7));

        // Atomic node range is 0..11, so it cannot commit when stable_through is 7
        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert_eq!(compiled.committable_prefix_rows, 0);
        assert!(matches!(compiled.commit[0], StreamRowCommit::Blocked));
    }

    #[test]
    fn trailing_list_with_ordinary_body_waits_for_egc_safety() {
        // "- item" is semantically determined, but the final `m` EGC remains
        // open. The Atomic node therefore cannot commit until a newline/seal.
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "- item");
        // NOT sealed

        let snap = stream.snapshot();
        assert_eq!(snap.source_end, StreamOffset::new(6));
        assert_eq!(snap.stable_through, StreamOffset::new(5));

        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert_eq!(compiled.committable_prefix_rows, 0);
        assert!(matches!(compiled.commit[0], StreamRowCommit::Blocked));
    }

    #[test]
    fn ordinary_list_becomes_committable_after_newline() {
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "- item\n");

        let snap = stream.snapshot();
        assert_eq!(snap.stable_through, StreamOffset::new(7));
        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert_eq!(compiled.committable_prefix_rows, 1);
        assert!(matches!(compiled.commit[0], StreamRowCommit::Atomic { .. }));
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
            assert_eq!(compiled_one.committable_prefix_rows, 0, "suffix={suffix:?}");
            let plan = plan_commit(&compiled_one, 10, None, StreamOffset::ZERO);
            assert_eq!(plan.payload.rows_to_write(), 0);
            assert_eq!(plan.next_committed_through, StreamOffset::ZERO);

            stream.push_delta(SegmentKind::Text, suffix);
            let phase_two = stream.snapshot();
            assert!(phase_two.validate());
            assert_eq!(phase_two.source_base, StreamOffset::ZERO);
            assert_eq!(
                phase_two.view.nodes[0].owned_range().start,
                StreamOffset::ZERO
            );
            assert_eq!(phase_two.view.nodes.len(), 1, "suffix={suffix:?}");
            assert!(
                matches!(
                    phase_two.view.nodes[0].provenance(),
                    StreamProvenance::Atomic(_) if suffix.starts_with('-') || suffix.starts_with('1')
                ) || matches!(
                    phase_two.view.nodes[0].provenance(),
                    StreamProvenance::Exact(_) if suffix.starts_with('#')
                )
            );
        }
    }

    #[test]
    fn mixed_atomic_exact_mutable_tail_stability() {
        // "**bold**\nplain\nmutable"
        // "**bold**\n" = 9 bytes, "plain\n" = 6 bytes, "mutable" = 7 bytes => total = 22
        // Atomic completed line (0..9) => stable
        // Exact completed line "plain\n" (9..15) => stable
        // Exact mutable tail "mutable" (15..22) => stability stops before trailing EGC
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "**bold**\nplain\nmutable");
        // NOT sealed

        let snap = stream.snapshot();
        assert!(snap.validate());
        assert_eq!(snap.source_end, StreamOffset::new(22));

        // stable_through must reach through both completed rows but stop before trailing EGC
        // append_only_text_stable_frontier("mutable", 15, false)
        // "mutable" graphemes: m(0) u(1) t(2) a(3) b(4) l(5) e(6) - last offset = 6
        // frontier = 15 + 6 = 21
        assert!(snap.stable_through >= StreamOffset::new(15));
        assert!(snap.stable_through < snap.source_end);

        // The Atomic completed line must be committable (stable_through >= 9)
        let compiled = compile_stream(&snap.view, 80, snap.stable_through);
        assert!(compiled.committable_prefix_rows >= 2); // Atomic row + Exact "plain" row
    }

    // --- Compaction & continuation regressions ---

    #[test]
    fn compact_before_preserves_atomic_transition_without_duplicating_committed_source() {
        // Phase 1: push "hello **bo"
        let mut stream = AssistantStream::new();
        stream.push_delta(SegmentKind::Text, "hello **bo");

        let snap1 = stream.snapshot();
        assert!(snap1.validate());
        assert_eq!(snap1.source_end, StreamOffset::new(10));
        assert_eq!(snap1.stable_through, StreamOffset::new(6)); // "hello "

        // Compile stream at width 6: "hello " wraps to row 0 and is committable prefix
        let compiled1 = compile_stream(&snap1.view, 6, snap1.stable_through);
        assert_eq!(compiled1.committable_prefix_rows, 1);
        assert_eq!(
            compiled1.commit[0],
            StreamRowCommit::Exact(StreamOffset::new(6))
        );

        // Apply compaction at 6 (as HostedStream does upon successful commit)
        stream.compact_before(StreamOffset::new(6));

        // Phase 2: push "ld**"
        stream.push_delta(SegmentKind::Text, "ld**");

        let snap2 = stream.snapshot();
        assert!(snap2.validate());
        assert_eq!(snap2.source_base, StreamOffset::new(6));
        assert_eq!(snap2.source_end, StreamOffset::new(14));

        // First node in snapshot must NOT start before 6: Atomic(6..14)
        assert_eq!(snap2.view.nodes.len(), 1);
        assert_eq!(
            snap2.view.nodes[0].owned_range(),
            StreamRange::new(StreamOffset::new(6), StreamOffset::new(14))
        );

        // Seal and plan remainder
        stream.seal();
        let snap3 = stream.snapshot();
        assert!(snap3.validate());
        assert_eq!(snap3.stable_through, StreamOffset::new(14));

        let compiled2 = compile_stream(&snap3.view, 80, snap3.stable_through);
        assert_eq!(compiled2.committable_prefix_rows, 1);
        assert!(matches!(
            compiled2.commit[0],
            StreamRowCommit::Atomic {
                source_end,
                ..
            } if source_end == StreamOffset::new(14)
        ));
    }

    #[test]
    fn wrapped_heading_continuation_retains_heading_styling_after_compaction() {
        let mut stream = AssistantStream::new();
        stream.push_delta(
            SegmentKind::Text,
            "## long heading that wraps across multiple physical rows",
        );

        let snap1 = stream.snapshot();
        assert!(snap1.validate());
        assert_eq!(snap1.view.nodes.len(), 1);

        // Compact before byte 10 (inside the heading)
        stream.compact_before(StreamOffset::new(10));

        let snap2 = stream.snapshot();
        assert!(snap2.validate());
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
        assert!(snap1.validate());
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
        assert!(snap2.validate());
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
        assert!(snap3.validate());
        assert_eq!(snap3.source_base, StreamOffset::new(7));
        let tail_doc3 = parse_assistant_tail(
            &slice_segments(&stream3.segments, 7, snap3.source_end.as_u64() as usize),
            StreamOffset::new(7),
            stream3.continuation,
        );
        assert_eq!(tail_doc3.rows[0].layout, AssistantRowLayout::Plain);
    }
}
