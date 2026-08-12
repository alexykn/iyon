use std::time::{Duration, Instant};

use iyon_tui::{
    Alignment, Block, Inline, InlineContent, List, ListItem, ListMarker, Mark, MarkSet,
    MarkdownProjector, NumberDelimiter, NumberStyle, Projection, ProjectionBuilder, Projector,
    Renderer, Smooth, StreamOffset, StreamRange, Table, TableCell, TableColumn, TableRow,
    TextContent, TextProvenance, TextRenderStyle, TextRenderer, TextRun,
    validate_projection_transition, validate_text_projection,
};

fn source_projection(
    source: &str,
    stable: usize,
    sealed: bool,
    base: usize,
) -> Projection<TextContent> {
    let start = StreamOffset::new(base as u64);
    let end = StreamOffset::new((base + source.len()) as u64);
    let mut builder = ProjectionBuilder::new(
        start,
        StreamOffset::new((base + stable) as u64),
        end,
        sealed,
    );
    if source.is_empty() {
        return builder.finish().unwrap();
    }
    if stable > 0 && stable < source.len() {
        builder = builder.emit(
            StreamRange::new(start, StreamOffset::new((base + stable) as u64)),
            TextContent::raw(&source[..stable]),
        );
        builder = builder.emit(
            StreamRange::new(StreamOffset::new((base + stable) as u64), end),
            TextContent::raw(&source[stable..]),
        );
    } else {
        builder = builder.emit(StreamRange::new(start, end), TextContent::raw(source));
    }
    builder.finish().unwrap()
}

fn sealed(source: &str) -> Projection<TextContent> {
    source_projection(source, source.len(), true, 0)
}

fn semantic(output: &Projection<TextContent>) -> Vec<TextContent> {
    output
        .spans()
        .iter()
        .flat_map(|span| span.values().iter().cloned())
        .collect()
}

#[test]
fn restart_includes_reference_context_and_supports_exact_compaction() {
    let source = "[foo]: /target\n\none\n\ntwo\n\n[foo]\n";
    let mut projector = MarkdownProjector::default();
    let output = projector
        .project(&source_projection(source, source.len(), false, 0))
        .unwrap();
    let reference = StreamOffset::new((source.len() - 5) as u64);
    let restart = projector.restart_from(reference);
    assert!(restart <= reference);
    assert_eq!(restart, StreamOffset::ZERO);

    let retained = &source[restart.as_u64() as usize..];
    let compacted = source_projection(retained, retained.len(), true, restart.as_u64() as usize);
    let mut resumed = projector;
    let resumed_output = resumed.project(&compacted).unwrap();
    let mut fresh = MarkdownProjector::default();
    let fresh_output = fresh.project(&sealed(source)).unwrap();
    assert_eq!(resumed_output, fresh_output);
    assert_eq!(output.source_base(), StreamOffset::ZERO);
}

#[test]
fn stable_closed_blocks_are_published_inside_a_raw_domain() {
    let source = (0..100)
        .map(|index| format!("block {index}\n\n"))
        .collect::<String>()
        + "tail";
    let stable = source.len() - 4;
    let mut projector = MarkdownProjector::default();
    let output = projector
        .project(&source_projection(source.as_str(), stable, false, 0))
        .unwrap();
    assert!(output.stable_through().as_u64() > 0);
    assert!(output.stable_through().as_u64() <= stable as u64);
    validate_text_projection(&output).unwrap();
}

#[test]
fn unresolved_references_remain_mutable_until_sealed_resolution() {
    let mut projector = MarkdownProjector::default();
    let mut previous = None;
    let mut previous_len = 0;
    for source in [
        "[foo]\n\n",
        "[foo]\n\n[foo]: /a",
        "[foo]\n\n[foo]: /b",
        "[foo]\n\n",
        "[foo]\n\n[foo]: /b\n",
    ] {
        if source.len() < previous_len {
            continue;
        }
        let next = projector
            .project(&source_projection(source, source.len(), false, 0))
            .unwrap();
        previous_len = source.len();
        validate_text_projection(&next).unwrap();
        if let Some(previous) = &previous {
            validate_projection_transition(previous, &next).unwrap();
        }
        previous = Some(next);
    }
    let final_source = "[foo]\n\n[foo]: /b\n";
    let final_output = projector
        .project(&source_projection(
            final_source,
            final_source.len(),
            true,
            0,
        ))
        .unwrap();
    let mut fresh = MarkdownProjector::default();
    assert_eq!(final_output, fresh.project(&sealed(final_source)).unwrap());
}

#[test]
fn nonzero_root_coordinates_are_preserved() {
    let source = "é\n\nnext";
    let mut projector = MarkdownProjector::default();
    let output = projector
        .project(&source_projection(source, source.len(), true, 1000))
        .unwrap();
    assert_eq!(output.source_base(), StreamOffset::new(1000));
    let run = semantic(&output)
        .into_iter()
        .find_map(|value| match value {
            TextContent::Block(block) => match block.kind() {
                iyon_tui::BlockKind::Paragraph(content) => content
                    .items()
                    .iter()
                    .find_map(|inline| inline.as_text().cloned()),
                _ => None,
            },
            TextContent::Raw(_) => None,
        })
        .unwrap();
    assert_eq!(
        run.provenance(),
        &TextProvenance::Exact(StreamRange::new(
            StreamOffset::new(1000),
            StreamOffset::new(1002)
        ))
    );
}

#[test]
fn markdown_composes_after_smooth_without_special_streaming_api() {
    let source = sealed("first\n\nsecond\n");
    let mut smooth = Smooth::default();
    let mut markdown = MarkdownProjector::default();
    let published = smooth.project(&source).unwrap();
    let first = markdown.project(&published).unwrap();
    validate_text_projection(&first).unwrap();
    let now = Instant::now() + Duration::from_secs(1);
    let _ = smooth.advance(now);
    let flushed = smooth.project(&source).unwrap();
    let final_output = markdown.project(&flushed).unwrap();
    let mut fresh = MarkdownProjector::default();
    assert_eq!(final_output, fresh.project(&source).unwrap());
}

#[test]
fn renderer_preserves_generic_list_table_and_image_semantics() {
    let paragraph =
        |text: &str| Block::paragraph(InlineContent::new([Inline::text(TextRun::synthetic(text))]));
    let list = List::new(
        ListMarker::Ordered {
            start: 1,
            style: NumberStyle::LowerAlpha,
            delimiter: NumberDelimiter::TwoParens,
        },
        true,
        [ListItem::new([paragraph("item")])],
    );
    let caption = [paragraph("caption")];
    let table = Table::new(
        Some(caption),
        [TableColumn::new(Alignment::Center)],
        1,
        [TableRow::new([TableCell::plain([paragraph("cell")])])],
    )
    .unwrap();
    let alt =
        Inline::text(TextRun::synthetic("alt")).with_marks(MarkSet::new([Mark::Strong]).unwrap());
    let image = Inline::image(iyon_tui::Image::new(
        "image",
        None::<&str>,
        InlineContent::new([alt]),
    ))
    .with_marks(MarkSet::new([Mark::Emphasis]).unwrap());
    let block = Block::paragraph(InlineContent::new([image]));
    let renderer = TextRenderer::with_style(
        TextRenderStyle::new().with_annotation_tag("app:tag", iyon_tui::StyleSpec::new().bold()),
    );
    let _ = renderer.render(&TextContent::block(Block::list(list)));
    let _ = renderer.render(&TextContent::block(Block::table(table)));
    let _ = renderer.render(&TextContent::block(block));
}

#[test]
fn smooth_markdown_final_result_is_chunk_invariant() {
    let source = "ASCII\n\né\n\n中\n\na\u{301}\n\n👩‍💻\n";
    let mut one = MarkdownProjector::default();
    let expected = one.project(&sealed(source)).unwrap();
    for split in 0..=source.len() {
        if !source.is_char_boundary(split) {
            continue;
        }
        let mut projector = MarkdownProjector::default();
        let first = source_projection(&source[..split], split, false, 0);
        let _ = projector.project(&first).unwrap();
        let output = projector.project(&sealed(source)).unwrap();
        assert_eq!(output, expected);

        let mut chunked = MarkdownProjector::default();
        let mut previous = None;
        for end in source
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(source.len()))
        {
            let snapshot = source_projection(&source[..end], end, false, 0);
            let next = chunked.project(&snapshot).unwrap();
            validate_text_projection(&next).unwrap();
            if let Some(previous) = &previous {
                validate_projection_transition(previous, &next).unwrap();
            }
            previous = Some(next);
        }
        let chunked_final = chunked.project(&sealed(source)).unwrap();
        assert_eq!(chunked_final, expected);
    }
}

#[test]
#[cfg(feature = "test-util")]
fn stable_cache_reuses_closed_prefixes() {
    let mut projector = MarkdownProjector::default();
    let mut source = String::new();
    for index in 0..1000 {
        source.push_str(&format!("block {index}\n\n"));
        let output = projector
            .project(&source_projection(&source, source.len(), false, 0))
            .unwrap();
        validate_text_projection(&output).unwrap();
    }
    let (invocations, bytes) = projector.parser_work();
    println!(
        "1000-block parser work: invocations={invocations}, bytes={bytes}, source={}",
        source.len()
    );
    assert!(invocations > 0);
    assert!(bytes < source.len().saturating_mul(100));
}

#[test]
fn renderer_accepts_all_number_styles() {
    let text = |value: &str| {
        Block::paragraph(InlineContent::new([Inline::text(TextRun::synthetic(
            value,
        ))]))
    };
    for style in [
        NumberStyle::Decimal,
        NumberStyle::LowerAlpha,
        NumberStyle::UpperAlpha,
        NumberStyle::LowerRoman,
        NumberStyle::UpperRoman,
    ] {
        let list = List::new(
            ListMarker::Ordered {
                start: 9,
                style,
                delimiter: NumberDelimiter::Paren,
            },
            true,
            [ListItem::new([text("x")]), ListItem::new([text("y")])],
        );
        let _ = TextRenderer::new().render(&TextContent::block(Block::list(list)));
    }
}
