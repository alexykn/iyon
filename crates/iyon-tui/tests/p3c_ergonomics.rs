use iyon_tui::projection::{ProjectionBuilder, validate_projection_relation};
use iyon_tui::stream::{
    StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamingSource, TextStream,
    append_only_text_stable_frontier,
};
use iyon_tui::text::{
    BlockKind, CodeBlock, HeadingLevel, InlineKind, LanguageId, List, ListItem, Mark, TextRun,
};
use iyon_tui::{Block, History, HistoryLayout, Inline, InlineContent, Insets, SmoothConfig};

#[test]
fn ordinary_text_construction_matches_explicit_ir() {
    let convenience = Block::paragraph("hello");
    let explicit = Block::paragraph(InlineContent::new([Inline::text(TextRun::synthetic(
        "hello",
    ))]));
    assert_eq!(convenience, explicit);

    assert_eq!(HeadingLevel::H1.get(), 1);
    assert_eq!(Inline::text("x").strong().marks().marks(), &[Mark::Strong]);
    assert_eq!(List::bulleted([ListItem::paragraph("item")]).tight(), true);

    let language = LanguageId::new("json").unwrap();
    let code = CodeBlock::new(Some(language), None::<&str>, "{}");
    assert!(matches!(Block::code(code).kind(), BlockKind::CodeBlock(_)));
    assert!(matches!(Inline::text("x").kind(), InlineKind::Text(_)));
}

#[test]
fn projection_helpers_preserve_envelope_and_relation() {
    let input = ProjectionBuilder::new(
        StreamOffset::new(10),
        StreamOffset::new(12),
        StreamOffset::new(12),
        true,
    )
    .emit(
        StreamRange::new(StreamOffset::new(10), StreamOffset::new(12)),
        7u8,
    )
    .finish()
    .unwrap();
    let mapped = input.map_ref(|value| value.to_string());
    assert_eq!(mapped.source_base(), input.source_base());
    assert_eq!(mapped.spans()[0].values(), &["7"]);
    validate_projection_relation(&input, &mapped).unwrap();

    let split = input.map_spans(|span| vec![*span.values().first().unwrap(), 8]);
    assert_eq!(split.spans().len(), 1);
    assert_eq!(split.spans()[0].values(), &[7, 8]);
}

#[test]
fn smooth_and_history_configuration_are_default_first() {
    assert_eq!(SmoothConfig::new(), SmoothConfig::default());
    assert_eq!(
        HistoryLayout::new()
            .with_padding(Insets::all(1))
            .with_gap(2),
        HistoryLayout::from_parts(Insets::all(1), 2)
    );
}

#[test]
fn text_stream_reuses_append_only_frontier_and_history_lifecycle() {
    let mut stream = TextStream::new();
    assert_eq!(stream.snapshot().source_end(), StreamOffset::ZERO);
    stream.push("a");
    stream.push("\u{301}");
    let snapshot = stream.snapshot();
    assert_eq!(
        snapshot.stable_through(),
        append_only_text_stable_frontier("a\u{301}", StreamOffset::ZERO, false)
    );
    assert_eq!(stream.revision(), StreamRevision::new(2));

    let mut history = History::new();
    let handle = history.push_stream(TextStream::new()).unwrap();
    history
        .update_stream(handle, |source| source.push("hello"))
        .unwrap();
    history
        .update_stream(handle, |source| source.push(" world"))
        .unwrap();
    history.seal_stream(handle).unwrap();
    history.push("done").unwrap();

    let mut compacted = TextStream::from_text("éx");
    compacted.seal();
    compacted.compact_before(StreamOffset::new(2));
    assert_eq!(compacted.source_base(), StreamOffset::new(2));
    assert_eq!(compacted.text(), "x");
}

#[test]
fn canonical_snapshot_builder_uses_a_range_and_fluent_stability() {
    let range = StreamRange::new(StreamOffset::new(4), StreamOffset::new(5));
    let snapshot = StreamSnapshot::builder(StreamRevision::ZERO, range)
        .fully_stable()
        .exact_text(range, [iyon_tui::TextSpan::plain("x")])
        .finish()
        .unwrap();
    assert_eq!(snapshot.source_base(), range.start());
    assert_eq!(snapshot.stable_through(), range.end());
}
