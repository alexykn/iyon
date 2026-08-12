use std::collections::BTreeMap;

use crate::{Block, BlockKind, BreakKind, Inline, InlineKind, IntoView, Mark, TextContent, View};

/// Converts a semantic value into the generic presentation [`View`].
///
/// Renderers do not receive terminal geometry, parser state, clocks, or
/// stream lifecycle. Width-dependent layout remains in the View pipeline.
pub trait Renderer<Input: ?Sized> {
    fn render(&self, input: &Input) -> View;
}

/// Controls generic text-to-View lowering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRenderStyle {
    block_gap: u16,
    soft_break: SoftBreakPolicy,
    heading: BTreeMap<u8, crate::StyleRef>,
    link: crate::StyleRef,
    code: crate::StyleRef,
    strikethrough: crate::StyleRef,
    annotation_tags: BTreeMap<String, crate::StyleRef>,
}

/// How a soft line break is presented as ordinary semantic text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoftBreakPolicy {
    Space,
    LineBreak,
}

impl Default for SoftBreakPolicy {
    fn default() -> Self {
        Self::Space
    }
}

impl Default for TextRenderStyle {
    fn default() -> Self {
        Self {
            block_gap: 1,
            soft_break: SoftBreakPolicy::default(),
            heading: BTreeMap::new(),
            link: crate::StyleSpec::new().underline().into(),
            code: crate::StyleSpec::new().into(),
            strikethrough: crate::StyleSpec::new().into(),
            annotation_tags: BTreeMap::new(),
        }
    }
}

impl TextRenderStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn block_gap(&self) -> u16 {
        self.block_gap
    }

    pub fn with_block_gap(mut self, gap: u16) -> Self {
        self.block_gap = gap;
        self
    }

    pub fn soft_break(&self) -> SoftBreakPolicy {
        self.soft_break
    }

    pub fn with_soft_break(mut self, policy: SoftBreakPolicy) -> Self {
        self.soft_break = policy;
        self
    }

    pub fn with_heading_style(mut self, level: u8, style: impl Into<crate::StyleRef>) -> Self {
        self.heading.insert(level, style.into());
        self
    }

    pub fn with_link_style(mut self, style: impl Into<crate::StyleRef>) -> Self {
        self.link = style.into();
        self
    }

    pub fn with_code_style(mut self, style: impl Into<crate::StyleRef>) -> Self {
        self.code = style.into();
        self
    }

    pub fn with_strikethrough_style(mut self, style: impl Into<crate::StyleRef>) -> Self {
        self.strikethrough = style.into();
        self
    }

    pub fn with_annotation_tag(
        mut self,
        tag: impl Into<String>,
        style: impl Into<crate::StyleRef>,
    ) -> Self {
        self.annotation_tags.insert(tag.into(), style.into());
        self
    }
}

/// The one generic renderer for the frozen text IR.
#[derive(Clone, Debug, Default)]
pub struct TextRenderer {
    style: TextRenderStyle,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_style(style: TextRenderStyle) -> Self {
        Self { style }
    }

    pub fn style(&self) -> &TextRenderStyle {
        &self.style
    }

    pub fn render_block(&self, block: &Block) -> View {
        let mut view = match block.kind() {
            BlockKind::Paragraph(content) => self.render_inline_content(content),
            BlockKind::Heading { level, content } => {
                let heading = self
                    .style
                    .heading
                    .get(&level.get())
                    .cloned()
                    .unwrap_or_else(|| crate::StyleSpec::new().bold().into());
                self.render_inline_content(content).style(heading)
            }
            BlockKind::BlockQuote { blocks } => {
                View::hanging("> ", "> ", self.render_blocks(blocks))
            }
            BlockKind::List(list) => self.render_list(list),
            BlockKind::CodeBlock(code) => View::text(code.body().text()).no_wrap().into_view(),
            BlockKind::Table(table) => self.render_table(table),
            BlockKind::ThematicBreak => View::text("───").no_wrap().into_view(),
            BlockKind::RawBlock { body, .. } => View::text(body.text()).no_wrap().into_view(),
            BlockKind::Container { blocks } => self.render_blocks(blocks),
        };
        let annotation_style = self.annotation_style(block.annotations());
        if annotation_style != crate::StyleSpec::default() {
            view = view.style(annotation_style);
        }
        view
    }

    fn render_blocks(&self, blocks: &[Block]) -> View {
        View::vertical(|column| {
            column.gap(self.style.block_gap);
            for block in blocks {
                column.child(self.render_block(block));
            }
        })
    }

    fn render_inline_content(&self, content: &crate::InlineContent) -> View {
        let mut spans = Vec::new();
        for inline in content.iter() {
            self.push_inline(&mut spans, inline);
        }
        View::styled_text(spans).into_view()
    }

    fn push_inline(&self, spans: &mut Vec<crate::TextSpan>, inline: &Inline) {
        let mut style = crate::StyleSpec::new();
        for mark in inline.marks().marks() {
            match mark {
                Mark::Strong => style = style.bold(),
                Mark::Emphasis => style = style.italic(),
                Mark::Underline => style = style.underline(),
                Mark::Link(_) => style.overlay(&self.style.link.local),
                Mark::Code => style.overlay(&self.style.code.local),
                Mark::Strikethrough => style.overlay(&self.style.strikethrough.local),
                Mark::Superscript | Mark::Subscript | Mark::SmallCaps => {}
            }
        }
        style.overlay(&self.annotation_style(inline.annotations()));
        match inline.kind() {
            InlineKind::Text(run) => {
                style.overlay(&self.annotation_style(run.annotations()));
                spans.push(crate::TextSpan::styled(run.text(), style));
            }
            InlineKind::Break(BreakKind::Soft) => match self.style.soft_break {
                SoftBreakPolicy::Space => spans.push(crate::TextSpan::plain(" ")),
                SoftBreakPolicy::LineBreak => spans.push(crate::TextSpan::plain("\n")),
            },
            InlineKind::Break(BreakKind::Hard) => spans.push(crate::TextSpan::plain("\n")),
            InlineKind::Image(image) => {
                for child in image.alt().iter() {
                    self.push_inline(spans, child);
                }
            }
            InlineKind::RawInline { body, .. } => {
                spans.push(crate::TextSpan::styled(body.text(), style));
            }
        }
    }

    fn annotation_style(&self, annotations: &crate::Annotations) -> crate::StyleSpec {
        let mut style = crate::StyleSpec::new();
        for tag in annotations.tags() {
            if let Some(patch) = self.style.annotation_tags.get(&tag.to_string()) {
                style.overlay(&patch.local);
            }
        }
        style
    }

    fn render_list(&self, list: &crate::List) -> View {
        View::vertical(|column| {
            column.gap(if list.tight() {
                0
            } else {
                self.style.block_gap
            });
            for (index, item) in list.items().iter().enumerate() {
                let marker = match list.marker() {
                    crate::ListMarker::Bullet => "- ".to_owned(),
                    crate::ListMarker::Ordered {
                        start,
                        delimiter: crate::NumberDelimiter::Period,
                        ..
                    } => format!("{}. ", start + index as u64),
                    crate::ListMarker::Ordered {
                        start,
                        delimiter: crate::NumberDelimiter::Paren,
                        ..
                    } => format!("{}) ", start + index as u64),
                    crate::ListMarker::Ordered {
                        start,
                        delimiter: crate::NumberDelimiter::TwoParens,
                        ..
                    } => format!("({}) ", start + index as u64),
                };
                let marker = match item.checked() {
                    Some(true) => format!("[x] {marker}"),
                    Some(false) => format!("[ ] {marker}"),
                    None => marker,
                };
                column.child(View::hanging(
                    marker.clone(),
                    " ".repeat(marker.chars().count()),
                    self.render_blocks(item.blocks()),
                ));
            }
        })
    }

    fn render_table(&self, table: &crate::Table) -> View {
        View::vertical(|column| {
            column.gap(self.style.block_gap);
            for (row_index, row) in table.rows().iter().enumerate() {
                column.child(View::horizontal(|horizontal| {
                    for cell in row.cells() {
                        let mut cell_view = self.render_blocks(cell.blocks());
                        if row_index < table.header_rows() {
                            cell_view = cell_view.bold();
                        }
                        horizontal.flex(cell_view);
                    }
                }));
            }
        })
    }
}

impl Renderer<TextContent> for TextRenderer {
    fn render(&self, input: &TextContent) -> View {
        match input {
            TextContent::Raw(raw) => View::text(raw.text()).into_view(),
            TextContent::Block(block) => self.render_block(block),
        }
    }
}

impl Renderer<Block> for TextRenderer {
    fn render(&self, input: &Block) -> View {
        self.render_block(input)
    }
}

impl Renderer<[TextContent]> for TextRenderer {
    fn render(&self, input: &[TextContent]) -> View {
        View::vertical(|column| {
            column.gap(self.style.block_gap);
            for content in input {
                column.child(Renderer::render(self, content));
            }
        })
    }
}
