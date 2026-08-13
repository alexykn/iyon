use iyon_tui::{
    Block, Component, HeadingLevel, History, Inline, MarkdownProjector, Projector,
    Renderer, Smooth, TextContent, TextOrigin, TextRenderer, TextSelector, TextStream,
};
use iyon_tui::projection::{ProjectionBuilder, ProjectionTransitionError, validate_projection_transition};
use iyon_tui::stream::{
    ProjectedHanging, ProjectedText, StreamOffset, StreamRange, StreamSnapshotBuilder,
    StreamingSource,
};
use iyon_tui::text::{BlockKind, LiteralText, Mark, TextRun, TextVisitor};

fn root_vocabulary() {
    let _: fn() -> History = History::new;
    let _: fn() -> TextStream = TextStream::new;
    let _: fn() -> MarkdownProjector = MarkdownProjector::default;
    let _: fn() -> Smooth = Smooth::default;
    let _ = TextRenderer::default().render(&TextContent::raw("x"));
    let _ = Block::heading(HeadingLevel::H1, "Heading");
    let _ = Inline::text("text");
    let _ = TextSelector::heading().origin(TextOrigin::MARKDOWN);
}

fn advanced_namespaces() {
    let _ = ProjectionBuilder::<u8>::new;
    let _ = ProjectionTransitionError::SourceEndRegressed;
    let _ = validate_projection_transition::<u8>;
    let _ = StreamOffset::ZERO;
    let _ = StreamRange::default();
    let _ = StreamSnapshotBuilder::new;
    let _ = ProjectedHanging::new(0, StreamRange::default(), "");
    let _ = ProjectedText::builder;
    let _ = BlockKind::ThematicBreak;
    let _ = LiteralText::from("x");
    let _ = TextRun::from("x");
    let _ = Mark::Strong;
}

fn main() {
    let _ = root_vocabulary as fn();
    let _ = advanced_namespaces as fn();
}

fn _trait_names<C: Component, P: Projector<TextContent>, R: Renderer<TextContent>, V: TextVisitor>() {}
fn _source_method<S: StreamingSource>(source: &S) { let _ = source.snapshot(); }
