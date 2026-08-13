use super::*;
use crate::transcript::pipeline::{assistant_renderer, assistant_view};
use iyon_tui::History;
use iyon_tui::stream::StreamingSource;
use iyon_tui::text::{Mark, TextProvenance, TextVisitor, walk_content};
use std::time::{Duration, Instant};

fn drain_pacing(source: &mut AssistantStream) {
    let mut now = Instant::now();
    for _ in 0..10_000 {
        now += Duration::from_millis(16);
        if !source.advance(now) && source.next_wakeup().is_none() {
            break;
        }
    }
}

#[derive(Default)]
struct Runs {
    text: String,
    thinking: Vec<String>,
    marked: Vec<(String, Vec<Mark>, bool)>,
    annotated_provenances: Vec<(TextProvenance, bool)>,
}

impl TextVisitor for Runs {
    fn visit_text_run(&mut self, run: &iyon_tui::text::TextRun) {
        self.text.push_str(run.text());
        let thinking = run
            .annotations()
            .tags()
            .iter()
            .any(|tag| tag.to_string() == "app:thinking");
        if thinking {
            self.thinking.push(run.text().to_owned());
        }
        self.annotated_provenances
            .push((run.provenance().clone(), thinking));
    }

    fn visit_inline(&mut self, inline: &iyon_tui::text::Inline) {
        if let iyon_tui::text::InlineKind::Text(run) = inline.kind() {
            self.marked.push((
                run.text().to_owned(),
                inline.marks().marks().to_vec(),
                run.annotations()
                    .tags()
                    .iter()
                    .any(|tag| tag.to_string() == "app:thinking"),
            ));
        }
        iyon_tui::text::walk_inline(self, inline);
    }
}

fn stream_with(chunks: &[(SegmentKind, &str)]) -> AssistantStream {
    let mut stream = AssistantStream::new();
    for &(kind, text) in chunks {
        stream.push_delta_paced(kind, text);
    }
    stream
}

#[test]
fn generic_pipeline_preserves_separator_and_thinking_annotation() {
    let mut stream = stream_with(&[
        (SegmentKind::Thinking, "**reason**"),
        (SegmentKind::Text, "answer"),
    ]);
    stream.seal();
    let mut runs = Runs::default();
    for span in stream.semantic.spans() {
        for value in span.values() {
            walk_content(&mut runs, value);
        }
    }
    assert!(runs.text.contains("reason"));
    assert!(runs.text.contains("answer"));
    assert_eq!(runs.thinking.concat(), "reason");
    assert_eq!(
        stream.received_end.as_u64(),
        "**reason**\n\nanswer".len() as u64
    );
}

#[test]
fn sealed_snapshot_owns_all_source_and_is_fully_stable() {
    let mut stream = stream_with(&[(SegmentKind::Text, "# title\n\nbody **bold**")]);
    stream.seal();
    let snapshot = stream.snapshot();
    assert_eq!(snapshot.source_base(), StreamOffset::ZERO);
    assert_eq!(
        snapshot.source_end().as_u64(),
        "# title\n\nbody **bold**".len() as u64
    );
    assert_eq!(snapshot.stable_through(), snapshot.source_end());
    assert!(stream.is_sealed());
}

#[test]
fn same_kind_chunking_converges_after_seal() {
    let mut split = stream_with(&[
        (SegmentKind::Text, "hello "),
        (SegmentKind::Text, "**world**"),
    ]);
    let mut whole = stream_with(&[(SegmentKind::Text, "hello **world**")]);
    split.seal();
    whole.seal();
    assert_eq!(split.semantic, whole.semantic);
    assert_eq!(split.snapshot(), whole.snapshot());
}

#[test]
fn stream_snapshot_and_final_assistant_view_compile_identically() {
    let renderer = assistant_renderer();
    for width in [1, 2, 4, 8, 20, 40, 80] {
        let mut stream = stream_with(&[(
            SegmentKind::Text,
            "# title\n\n- one\n- **two**\n\ninline `code`",
        )]);
        stream.seal();
        let snapshot = stream.snapshot();
        let stream_view = snapshot.view_for_test();
        let final_view = assistant_view(&stream.semantic, &renderer);
        assert_eq!(
            iyon_tui::testing::compile_view_lines(&stream_view, width),
            iyon_tui::testing::compile_view_lines(&final_view, width),
            "physical mismatch at width {width}"
        );
    }
}

#[test]
fn thinking_composition_preserves_marks_and_splits_exact_provenance() {
    let mut stream = stream_with(&[
        (SegmentKind::Thinking, "**reason** and `code`"),
        (SegmentKind::Text, "answer"),
    ]);
    stream.seal();
    let mut runs = Runs::default();
    for span in stream.semantic.spans() {
        for value in span.values() {
            walk_content(&mut runs, value);
        }
    }
    assert!(runs.text.contains("reason"));
    assert!(runs.text.contains("code"));
    assert!(runs.marked.iter().any(|(text, marks, thinking)| {
        text == "r" && marks.contains(&Mark::Strong) && *thinking
    }));
    assert!(runs.marked.iter().any(|(text, marks, thinking)| {
        text == "code" && marks.contains(&Mark::Code) && *thinking
    }));
    assert!(
        runs.marked
            .iter()
            .any(|(text, _, thinking)| { text == "a" && !*thinking })
    );
    let thinking_range = StreamRange::new(StreamOffset::ZERO, StreamOffset::new(21));
    let answer_start = thinking_range.end().saturating_add(2);
    let answer_range = StreamRange::new(answer_start, stream.received_end);
    assert!(runs.annotated_provenances.iter().any(|(provenance, thinking)| {
        *thinking
            && matches!(provenance, TextProvenance::Exact(range) if range.start() >= thinking_range.start() && range.end() <= thinking_range.end())
    }));
    assert!(runs.annotated_provenances.iter().any(|(provenance, thinking)| {
        !*thinking
            && matches!(provenance, TextProvenance::Exact(range) if range.start() >= answer_range.start() && range.end() <= answer_range.end())
    }));
}

#[test]
fn temporal_publication_has_backlog_and_seal_flushes_all_source() {
    let mut stream = AssistantStream::new();
    stream.push_delta_paced(SegmentKind::Text, "setext\n====");
    let received = stream.received_end;
    let before = stream.snapshot();
    assert!(before.source_end() < received);
    let first = std::time::Instant::now();
    assert!(stream.next_wakeup().is_some() || stream.snapshot().source_end() > StreamOffset::ZERO);
    let _ = stream.advance(first);
    let due = stream.next_wakeup().unwrap_or(first);
    let _ = stream.advance(due + Duration::from_secs(1));
    let after = stream.snapshot();
    assert!(after.source_end() > before.source_end());
    assert!(after.source_end() <= received);
    assert!(after.stable_through() >= before.stable_through());
    stream.seal();
    assert_eq!(stream.snapshot().source_end(), received);
    assert_eq!(stream.snapshot().stable_through(), received);
}

#[test]
fn live_history_compaction_preserves_published_suffix() {
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    history
        .update_stream(handle, |source| {
            source.push_delta_paced(SegmentKind::Text, "first paragraph\n\n");
            drain_pacing(source);
        })
        .unwrap();
    history
        .update_stream(handle, |source| {
            source.push_delta_paced(SegmentKind::Text, "second paragraph\n\n");
        })
        .expect("live compaction must preserve the unpublished semantic suffix");
    history
        .update_stream(handle, |source| {
            drain_pacing(source);
            source.push_delta_paced(SegmentKind::Text, "third paragraph");
        })
        .expect("later compaction must keep later suffix nodes");
    history
        .seal_stream(handle)
        .expect("seal after live compaction must succeed");
}

#[test]
fn reference_compaction_respects_markdown_restart_floor() {
    let source = "[foo]: /target\n\nearlier paragraph\n\nlater [foo]";
    let mut stream = stream_with(&[(SegmentKind::Text, source)]);
    stream.seal();
    let before = stream.semantic.clone();
    let later = StreamOffset::new(source.find("later").expect("later block") as u64);
    StreamingSource::compact_before(&mut stream, later);
    let base = stream.snapshot().source_base();
    assert!(base < later, "reference context must pull restart backward");
    let expected: Vec<_> = before
        .spans()
        .iter()
        .filter(|span| span.source().end() > base)
        .map(|span| (span.source(), span.values().to_vec()))
        .collect();
    let actual: Vec<_> = stream
        .semantic
        .spans()
        .iter()
        .map(|span| (span.source(), span.values().to_vec()))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn empty_and_whitespace_sources_have_valid_snapshots() {
    for source in ["", " ", "\n", "\n\n", "   \n"] {
        let mut stream = stream_with(&[(SegmentKind::Text, source)]);
        stream.seal();
        let snapshot = stream.snapshot();
        assert_eq!(snapshot.source_end().as_u64(), source.len() as u64);
        assert_eq!(snapshot.stable_through(), snapshot.source_end());
    }
}
