use super::*;
use crate::transcript::pipeline::assistant_renderer;
use iyon_tui::stream::StreamingSource;
use iyon_tui::text::{TextVisitor, walk_content};

#[derive(Default)]
struct Runs {
    text: String,
    thinking: Vec<String>,
}

impl TextVisitor for Runs {
    fn visit_text_run(&mut self, run: &iyon_tui::text::TextRun) {
        self.text.push_str(run.text());
        if run
            .annotations()
            .tags()
            .iter()
            .any(|tag| tag.to_string() == "app:thinking")
        {
            self.thinking.push(run.text().to_owned());
        }
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
fn snapshot_uses_generic_renderer_for_blocks() {
    let mut stream = stream_with(&[(SegmentKind::Text, "one\n\ntwo")]);
    stream.seal();
    let rendered =
        crate::transcript::pipeline::assistant_view(&stream.semantic, &assistant_renderer());
    let _ = rendered;
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
