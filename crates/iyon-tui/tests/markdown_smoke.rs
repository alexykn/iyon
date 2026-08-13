use iyon_tui::projection::ProjectionBuilder;
use iyon_tui::stream::{StreamOffset, StreamRange};
use iyon_tui::text::{BlockKind, TextProvenance};
use iyon_tui::{MarkdownOptions, MarkdownProjector, Projection, Projector, RawText, TextContent};

fn projection(source: &str, sealed: bool) -> Projection<TextContent> {
    let end = StreamOffset::new(source.len() as u64);
    ProjectionBuilder::new(StreamOffset::ZERO, end, end, sealed)
        .emit(
            StreamRange::new(StreamOffset::ZERO, end),
            TextContent::Raw(RawText::new(source)),
        )
        .finish()
        .unwrap()
}

fn values(output: &Projection<TextContent>) -> Vec<&TextContent> {
    output
        .spans()
        .iter()
        .flat_map(|span| span.values())
        .collect()
}

#[test]
fn markdown_converts_common_blocks_and_exact_code_body() {
    let source = "# Title\n\n**hello** _world_\n\n```json\n{\"ok\":true}\n```\n";
    let mut projector = MarkdownProjector::new(MarkdownOptions::commonmark());
    let output = projector.project(&projection(source, true)).unwrap();
    assert!(output.is_sealed());
    assert_eq!(output.stable_through(), output.source_end());
    assert!(values(&output).iter().any(|value| matches!(value, TextContent::Block(block) if matches!(block.kind(), BlockKind::Heading { .. }))));
    let code = values(&output)
        .into_iter()
        .find_map(|value| match value {
            TextContent::Block(block) => block.as_code_block(),
            TextContent::Raw(_) => None,
        })
        .unwrap();
    assert_eq!(code.language().unwrap().as_str(), "json");
    assert_eq!(code.body().text(), "{\"ok\":true}\n");
    assert!(matches!(
        code.body().runs()[0].provenance(),
        TextProvenance::Exact(_)
    ));
}

#[test]
fn markdown_extensions_are_independent() {
    let source = "- [x] done\n\n~~gone~~\n\n| a | b |\n|---|---|\n| c | d |\n";
    for options in [
        MarkdownOptions::commonmark(),
        MarkdownOptions::commonmark().with_task_lists(true),
        MarkdownOptions::commonmark().with_strikethrough(true),
        MarkdownOptions::commonmark().with_tables(true),
        MarkdownOptions::commonmark()
            .with_task_lists(true)
            .with_strikethrough(true)
            .with_tables(true),
    ] {
        let mut projector = MarkdownProjector::new(options);
        projector.project(&projection(source, true)).unwrap();
    }
}

#[test]
fn plain_projector_claims_raw_domains_and_respects_barriers() {
    let source = "one\n\ntwo";
    let mut projector = iyon_tui::PlainTextProjector::new();
    let output = projector.project(&projection(source, true)).unwrap();
    assert_eq!(values(&output).len(), 1);
    assert!(
        matches!(values(&output)[0], TextContent::Block(block) if matches!(block.kind(), BlockKind::Paragraph(_)))
    );
}
