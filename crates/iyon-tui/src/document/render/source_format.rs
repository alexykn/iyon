use super::{Renderer, TextRenderer};
use crate::document::origin::stamp_block_origin;
use crate::document::{
    Block, BlockKind, HeadingLevel, Inline, InlineContent, List, ListItem, Table, TableCell,
    TableColumn, TableRow, TextContent, TextOrigin, TextProjectionError, TextSelector,
    validate_text_projection,
};
use crate::physical::PhysicalStyle;
use crate::presentation::layout::compile_view_with_theme;
use crate::projection::{Projection, ProjectionBuilder, Projector};
use crate::stream::{StreamOffset, StreamRange};
use crate::{StyleSpec, Theme, View};

fn style_at(view: &View, theme: &Theme, needle: &str) -> PhysicalStyle {
    let painted = compile_view_with_theme(view, 80, theme);
    for row in &painted.rows {
        if let Some(index) = row.cell_x_of(needle) {
            return row.style_at(index).expect("painted cell");
        }
    }
    panic!(
        "did not find {needle:?} in {:?}",
        painted
            .rows
            .iter()
            .map(|row| row.plain_text())
            .collect::<Vec<_>>()
    );
}

fn raw(source: &str) -> Projection<TextContent> {
    let end = StreamOffset::new(source.len() as u64);
    ProjectionBuilder::new(StreamOffset::ZERO, end, end, true)
        .emit(
            StreamRange::new(StreamOffset::ZERO, end),
            TextContent::raw(source),
        )
        .finish()
        .unwrap()
}

struct FakeAsciiDocProjector {
    origin: TextOrigin,
}

impl FakeAsciiDocProjector {
    fn new(origin: TextOrigin) -> Self {
        Self { origin }
    }
}

impl Projector<TextContent> for FakeAsciiDocProjector {
    type Output = TextContent;
    type Error = TextProjectionError;

    fn project(
        &mut self,
        input: &Projection<TextContent>,
    ) -> Result<Projection<Self::Output>, Self::Error> {
        validate_text_projection(input)?;
        let stable = if input.is_sealed() {
            input.source_end()
        } else {
            input.source_base()
        };
        let mut builder = ProjectionBuilder::new(
            input.source_base(),
            stable,
            input.source_end(),
            input.is_sealed(),
        );
        for span in input.spans() {
            match span.values() {
                [TextContent::Raw(raw)] => {
                    let text = raw.text();
                    let block = if let Some(title) = text.strip_prefix("= ") {
                        Block::heading(HeadingLevel::H1, title.trim_end())
                    } else {
                        Block::paragraph(text.trim_end())
                    };
                    builder = builder.emit(
                        span.source(),
                        TextContent::block(stamp_block_origin(block, &self.origin)),
                    );
                }
                values => {
                    builder = builder.emit_many(span.source(), values.iter().cloned());
                }
            }
        }
        builder.finish().map_err(TextProjectionError::Projection)
    }
}

#[test]
fn hand_built_non_markdown_origin_uses_generic_defaults_and_optional_specialization() {
    let asciidoc = TextOrigin::new("asciidoc").unwrap();
    let heading = Block::heading(
        HeadingLevel::H1,
        InlineContent::new([Inline::text("hello").strong()]),
    )
    .with_origin(asciidoc.clone());
    let list =
        Block::list(List::bulleted([ListItem::paragraph("item")])).with_origin(asciidoc.clone());
    let table = Block::table(
        Table::new(
            None::<Vec<Block>>,
            [TableColumn::start(), TableColumn::start()],
            1,
            [
                TableRow::new([TableCell::text("Head"), TableCell::text("Col")]),
                TableRow::new([TableCell::text("A"), TableCell::text("B")]),
            ],
        )
        .unwrap(),
    )
    .with_origin(asciidoc.clone());

    let renderer = TextRenderer::new();
    let empty = Theme::new();
    assert!(style_at(&renderer.render(&heading), &empty, "hello").bold);
    assert!(style_at(&renderer.render(&heading), &empty, "hello").underline);
    let _ = style_at(&renderer.render(&list), &empty, "item");
    assert!(style_at(&renderer.render(&table), &empty, "Head").bold);
    assert!(!style_at(&renderer.render(&table), &empty, "A").bold);

    let specialized = Theme::new().with_text_style(
        TextSelector::heading().origin(asciidoc.clone()),
        StyleSpec::new().italic(),
    );
    assert!(style_at(&renderer.render(&heading), &specialized, "hello").italic);
    let markdown_heading = Block::heading(
        HeadingLevel::H1,
        InlineContent::new([Inline::text("hello").strong()]),
    )
    .with_origin(TextOrigin::MARKDOWN);
    assert!(!style_at(&renderer.render(&markdown_heading), &specialized, "hello").italic);
}

#[test]
fn fake_asciidoc_projector_needs_no_new_theme_or_renderer() {
    let origin = TextOrigin::new("asciidoc").unwrap();
    let mut projector = FakeAsciiDocProjector::new(origin.clone());
    let output = projector.project(&raw("= Heading\n")).unwrap();
    let heading = output
        .spans()
        .iter()
        .flat_map(|span| span.values())
        .find_map(|value| match value {
            TextContent::Block(block) => Some(block),
            TextContent::Raw(_) => None,
        })
        .unwrap();
    assert!(matches!(
        heading.kind(),
        BlockKind::Heading {
            level,
            ..
        } if *level == HeadingLevel::H1
    ));
    assert_eq!(heading.origin(), Some(origin.clone()));

    let renderer = TextRenderer::new();
    let empty = Theme::new();
    let view = renderer.render(heading);
    assert!(style_at(&view, &empty, "Heading").bold);
    assert!(style_at(&view, &empty, "Heading").underline);

    let specialized = Theme::new().with_text_style(
        TextSelector::heading().origin(origin),
        StyleSpec::new().italic(),
    );
    assert!(style_at(&view, &specialized, "Heading").italic);
    let markdown = renderer
        .render(&Block::heading(HeadingLevel::H1, "Heading").with_origin(TextOrigin::MARKDOWN));
    assert!(!style_at(&markdown, &specialized, "Heading").italic);
}
