use std::convert::Infallible;

use iyon_tui::{
    Block, CodeBlock, Inline, InlineContent, LanguageId, LiteralText, MarkdownProjector,
    Projection, ProjectionBuilder, Projector, ProjectorExt, RawText, Renderer, SemanticTag, Smooth,
    StreamOffset, StreamRange, TextContent, TextProvenance, TextRenderer, TextRewriter, TextRun,
    validate_text_projection, walk_rewrite_block,
};

fn raw(source: &str, sealed: bool) -> Projection<TextContent> {
    let end = StreamOffset::new(source.len() as u64);
    ProjectionBuilder::new(StreamOffset::ZERO, end, end, sealed)
        .emit(
            StreamRange::new(StreamOffset::ZERO, end),
            TextContent::raw(source),
        )
        .finish()
        .unwrap()
}

struct Syntax;
impl TextRewriter for Syntax {
    type Error = Infallible;
    fn rewrite_block(&mut self, block: Block) -> Result<Block, Self::Error> {
        walk_rewrite_block(self, block)
    }
    fn rewrite_literal(&mut self, literal: LiteralText) -> Result<LiteralText, Self::Error> {
        let tag = SemanticTag::new("syntax", "token").unwrap();
        Ok(LiteralText::new(literal.runs().iter().cloned().map(
            |run| {
                if run.text().contains("graph") || run.text().contains("ok") {
                    run.map_annotations(|annotations| annotations.with_tag(tag.clone()))
                } else {
                    run
                }
            },
        )))
    }
}

struct Json;
impl TextRewriter for Json {
    type Error = Infallible;
    fn rewrite_block(&mut self, block: Block) -> Result<Block, Self::Error> {
        let Some(code) = block.as_code_block() else {
            return walk_rewrite_block(self, block);
        };
        if code.language().map(LanguageId::as_str) != Some("json") {
            return walk_rewrite_block(self, block);
        }
        let Some(first) = code.body().runs().first() else {
            return Ok(block);
        };
        let start = match first.provenance() {
            TextProvenance::Exact(range) | TextProvenance::Derived(range) => range.start(),
            TextProvenance::Synthetic => return Ok(block),
        };
        let body = LiteralText::new([
            TextRun::synthetic("{\n  "),
            TextRun::exact("\"ok\"", StreamRange::new(start, start.saturating_add(4))).unwrap(),
            TextRun::synthetic(": true\n}"),
        ]);
        Ok(Block::code(CodeBlock::new(
            code.language().cloned(),
            code.info(),
            body,
        )))
    }
}

struct Mermaid;
impl TextRewriter for Mermaid {
    type Error = Infallible;
    fn rewrite_block(&mut self, block: Block) -> Result<Block, Self::Error> {
        let Some(code) = block.as_code_block() else {
            return walk_rewrite_block(self, block);
        };
        if code.language().map(LanguageId::as_str) != Some("mermaid") {
            return walk_rewrite_block(self, block);
        }
        let paragraph = Block::paragraph(InlineContent::new([Inline::text(TextRun::synthetic(
            code.body().text(),
        ))]));
        Ok(Block::container([paragraph]).with_annotations(
            block
                .annotations()
                .clone()
                .with_tag(SemanticTag::new("diagram", "mermaid").unwrap()),
        ))
    }
}

fn rewrite<P: TextRewriter<Error = Infallible>>(
    mut rewriter: P,
    input: &Projection<TextContent>,
) -> Projection<TextContent> {
    let mut output = ProjectionBuilder::new(
        input.source_base(),
        input.stable_through(),
        input.source_end(),
        input.is_sealed(),
    );
    for span in input.spans() {
        let values = span
            .values()
            .iter()
            .cloned()
            .map(|value| match value {
                TextContent::Raw(raw) => Ok(TextContent::Raw(raw)),
                TextContent::Block(block) => rewriter.rewrite_block(block).map(TextContent::Block),
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        output = output.emit_many(span.source(), values);
    }
    output.finish().unwrap()
}

#[test]
fn markdown_creates_real_json_syntax_and_mermaid_portals() {
    let source =
        "before\n\n```json\n{\"ok\":true}\n```\n\n```mermaid\ngraph TD\nA --> B\n```\n\nafter\n";
    let mut markdown = MarkdownProjector::default();
    let document = markdown.project(&raw(source, true)).unwrap();
    let json = rewrite(Json, &document);
    let syntax = rewrite(Syntax, &json);
    let mermaid = rewrite(Mermaid, &syntax);
    validate_text_projection(&mermaid).unwrap();
    assert!(
        mermaid
            .spans()
            .iter()
            .any(|span| span.values().iter().any(|value| {
                matches!(value, TextContent::Block(block) if block.as_container().is_some())
            }))
    );
    let renderer = TextRenderer::new();
    for span in mermaid.spans() {
        for value in span.values() {
            let _ = renderer.render(value);
        }
    }
}

struct Claim;
impl Projector<RawText> for Claim {
    type Output = TextContent;
    type Error = Infallible;
    fn project(
        &mut self,
        input: &Projection<RawText>,
    ) -> Result<Projection<TextContent>, Self::Error> {
        let mut output = ProjectionBuilder::new(
            input.source_base(),
            input.stable_through(),
            input.source_end(),
            input.is_sealed(),
        );
        output = output.emit(
            StreamRange::new(input.source_base(), input.source_base().saturating_add(5)),
            TextContent::block(Block::paragraph(InlineContent::new([Inline::text(
                TextRun::synthetic("claim"),
            )]))),
        );
        output = output.emit(
            StreamRange::new(input.source_base().saturating_add(5), input.source_end()),
            TextContent::raw("tail"),
        );
        Ok(output.finish().unwrap())
    }
}

#[test]
fn markdown_preserves_reverse_order_structured_barriers() {
    let source = "claimtail";
    let mut pipeline = Claim.then(MarkdownProjector::default());
    let output = pipeline
        .project(
            &ProjectionBuilder::new(
                StreamOffset::ZERO,
                StreamOffset::new(source.len() as u64),
                StreamOffset::new(source.len() as u64),
                true,
            )
            .emit(
                StreamRange::new(StreamOffset::ZERO, StreamOffset::new(source.len() as u64)),
                RawText::new(source),
            )
            .finish()
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        output.spans()[0].values()[0],
        TextContent::Block(_)
    ));
    assert!(matches!(
        output.spans()[1].values()[0],
        TextContent::Block(_)
    ));
}

#[test]
fn smooth_is_a_normal_upstream_projector() {
    let source = raw("one\n\ntwo\n", true);
    let mut pipeline = Smooth::default().then(MarkdownProjector::default());
    let output = pipeline.project(&source).unwrap();
    validate_text_projection(&output).unwrap();
}
