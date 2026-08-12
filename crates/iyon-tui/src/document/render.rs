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
            BlockKind::Paragraph(content) => self.render_inline_content(content).into_view(),
            BlockKind::Heading { level, content } => {
                let heading = self
                    .style
                    .heading
                    .get(&level.get())
                    .cloned()
                    .unwrap_or_else(|| crate::StyleSpec::new().bold().into());
                self.render_inline_content(content)
                    .style(heading)
                    .into_view()
            }
            BlockKind::BlockQuote { blocks } => {
                View::hanging("> ", "> ", self.render_blocks(blocks))
            }
            BlockKind::List(list) => self.render_list(list),
            BlockKind::CodeBlock(code) => self
                .render_literal(&code.body())
                .no_wrap()
                .style(self.style.code.clone())
                .into_view(),
            BlockKind::Table(table) => self.render_table(table),
            BlockKind::ThematicBreak => View::text("───").no_wrap().into_view(),
            BlockKind::RawBlock { body, .. } => self.render_literal(body).no_wrap().into_view(),
            BlockKind::Container { blocks } => self.render_blocks(blocks),
        };
        let annotation_style = self.annotation_style(block.annotations());
        if annotation_style != crate::StyleRef::default() {
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

    fn render_literal(&self, literal: &crate::LiteralText) -> crate::Text {
        let mut spans = Vec::new();
        for run in literal.runs() {
            let mut style = crate::StyleRef::default();
            let run_annotations = self.annotation_style(run.annotations());
            overlay_style(&mut style, &run_annotations);
            spans.push(crate::TextSpan::styled(run.text(), style));
        }
        View::styled_text(spans)
    }

    fn render_inline_content(&self, content: &crate::InlineContent) -> crate::Text {
        let mut spans = Vec::new();
        for inline in content.iter() {
            self.push_inline(&mut spans, inline, crate::StyleRef::default());
        }
        View::styled_text(spans)
    }

    fn push_inline(
        &self,
        spans: &mut Vec<crate::TextSpan>,
        inline: &Inline,
        inherited: crate::StyleRef,
    ) {
        let mut style = inherited;
        for mark in inline.marks().marks() {
            match mark {
                Mark::Strong => style.local = style.local.clone().bold(),
                Mark::Emphasis => style.local = style.local.clone().italic(),
                Mark::Underline => style.local = style.local.clone().underline(),
                Mark::Link(_) => overlay_style(&mut style, &self.style.link),
                Mark::Code => overlay_style(&mut style, &self.style.code),
                Mark::Strikethrough => overlay_style(&mut style, &self.style.strikethrough),
                Mark::Superscript | Mark::Subscript | Mark::SmallCaps => {}
            }
        }
        let inline_annotations = self.annotation_style(inline.annotations());
        overlay_style(&mut style, &inline_annotations);
        match inline.kind() {
            InlineKind::Text(run) => {
                let run_annotations = self.annotation_style(run.annotations());
                overlay_style(&mut style, &run_annotations);
                spans.push(crate::TextSpan::styled(run.text(), style));
            }
            InlineKind::Break(BreakKind::Soft) => match self.style.soft_break {
                SoftBreakPolicy::Space => spans.push(crate::TextSpan::plain(" ")),
                SoftBreakPolicy::LineBreak => spans.push(crate::TextSpan::plain("\n")),
            },
            InlineKind::Break(BreakKind::Hard) => spans.push(crate::TextSpan::plain("\n")),
            InlineKind::Image(image) => {
                for child in image.alt().iter() {
                    self.push_inline(spans, child, style.clone());
                }
            }
            InlineKind::RawInline { body, .. } => {
                for run in body.runs() {
                    let mut run_style = style.clone();
                    let run_annotations = self.annotation_style(run.annotations());
                    overlay_style(&mut run_style, &run_annotations);
                    spans.push(crate::TextSpan::styled(run.text(), run_style));
                }
            }
        }
    }

    fn annotation_style(&self, annotations: &crate::Annotations) -> crate::StyleRef {
        let mut style = crate::StyleRef::default();
        for tag in annotations.tags() {
            if let Some(patch) = self.style.annotation_tags.get(&tag.to_string()) {
                overlay_style(&mut style, patch);
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
                        style,
                        delimiter,
                    } => format_marker(start.saturating_add(index as u64), style, delimiter),
                };
                let marker = match item.checked() {
                    Some(true) => format!("[x] {marker}"),
                    Some(false) => format!("[ ] {marker}"),
                    None => marker,
                };
                let mut item_view = View::hanging(
                    marker.clone(),
                    " ".repeat(marker.chars().count()),
                    self.render_blocks(item.blocks()),
                );
                let annotation_style = self.annotation_style(item.annotations());
                if annotation_style != crate::StyleRef::default() {
                    item_view = item_view.style(annotation_style);
                }
                column.child(item_view);
            }
        })
    }

    fn render_table(&self, table: &crate::Table) -> View {
        View::vertical(|column| {
            column.gap(self.style.block_gap);
            if let Some(caption) = table.caption() {
                column.child(self.render_blocks(caption));
            }
            for (row_index, row) in table.rows().iter().enumerate() {
                let mut row_view = View::horizontal(|horizontal| {
                    for (column_index, cell) in row.cells().iter().enumerate() {
                        let mut cell_view = self.render_cell(cell, table, column_index);
                        if row_index < table.header_rows() {
                            cell_view = cell_view.bold();
                        }
                        horizontal.flex(cell_view);
                    }
                });
                let annotation_style = self.annotation_style(row.annotations());
                if annotation_style != crate::StyleRef::default() {
                    row_view = row_view.style(annotation_style);
                }
                column.child(row_view);
            }
        })
    }

    fn render_cell(
        &self,
        cell: &crate::TableCell,
        table: &crate::Table,
        column_index: usize,
    ) -> View {
        let mut view = if cell.blocks().len() == 1 {
            match cell.blocks()[0].kind() {
                BlockKind::Paragraph(content) => {
                    let alignment = cell.alignment().or_else(|| {
                        table
                            .columns()
                            .get(column_index)
                            .map(|column| column.alignment())
                    });
                    let text = alignment.map_or_else(
                        || self.render_inline_content(content),
                        |alignment| {
                            self.render_inline_content(content)
                                .text_align(to_horizontal_align(alignment))
                        },
                    );
                    text.into_view()
                }
                _ => self.render_blocks(cell.blocks()),
            }
        } else {
            self.render_blocks(cell.blocks())
        };
        let annotation_style = self.annotation_style(cell.annotations());
        if annotation_style != crate::StyleRef::default() {
            view = view.style(annotation_style);
        }
        view
    }
}

fn overlay_style(target: &mut crate::StyleRef, incoming: &crate::StyleRef) {
    if target.theme.is_none() {
        if let Some(theme) = &incoming.theme {
            target.theme = Some(theme.clone());
        }
    }
    target.local.overlay(&incoming.local);
}

fn format_marker(
    value: u64,
    style: crate::NumberStyle,
    delimiter: crate::NumberDelimiter,
) -> String {
    let number = format_number(value, style);
    match delimiter {
        crate::NumberDelimiter::Period => format!("{number}. "),
        crate::NumberDelimiter::Paren => format!("{number}) "),
        crate::NumberDelimiter::TwoParens => format!("({number}) "),
    }
}

fn format_number(value: u64, style: crate::NumberStyle) -> String {
    match style {
        crate::NumberStyle::Decimal => value.to_string(),
        crate::NumberStyle::LowerAlpha => alpha_number(value, b'a'),
        crate::NumberStyle::UpperAlpha => alpha_number(value, b'A'),
        crate::NumberStyle::LowerRoman => roman_number(value).to_lowercase(),
        crate::NumberStyle::UpperRoman => roman_number(value),
    }
}

fn alpha_number(mut value: u64, first: u8) -> String {
    if value == 0 {
        return String::new();
    }
    let mut result = Vec::new();
    while value != 0 {
        value -= 1;
        result.push((first + (value % 26) as u8) as char);
        value /= 26;
    }
    result.into_iter().rev().collect()
}

fn roman_number(mut value: u64) -> String {
    const VALUES: &[(u64, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut result = String::new();
    for &(unit, text) in VALUES {
        while value >= unit {
            value -= unit;
            result.push_str(text);
        }
    }
    result
}

fn to_horizontal_align(alignment: crate::Alignment) -> crate::HorizontalAlign {
    match alignment {
        crate::Alignment::Default | crate::Alignment::Start => crate::HorizontalAlign::Start,
        crate::Alignment::Center => crate::HorizontalAlign::Center,
        crate::Alignment::End => crate::HorizontalAlign::End,
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
