use super::super::{
    Alignment, Block, BlockKind, HeadingLevel, InlineContent, List, ListItem, ListMarker,
    NumberDelimiter, NumberStyle, Table, TableCell, TextListKind, TextPart, TextRole,
    TextTableSection, TextTaskState,
};
use super::TextRenderer;
use super::identity::{RenderContext, part_facts, semantic_view_facts, stamp_text, stamp_view};
use crate::{HorizontalAlign, View};

impl TextRenderer {
    pub(super) fn lower_block(&self, block: &Block, context: &RenderContext) -> View {
        let context = context.for_node(block.annotations());
        match block.kind() {
            BlockKind::Paragraph(content) => self.render_paragraph(block, content, &context, None),
            BlockKind::Heading { level, content } => {
                self.render_heading(block, *level, content, &context)
            }
            BlockKind::BlockQuote { blocks } => self.render_quote(block, blocks, &context),
            BlockKind::List(list) => self.render_list(block, list, &context),
            BlockKind::CodeBlock(code) => {
                let mut facts =
                    semantic_view_facts(&context, TextRole::CodeBlock, block.annotations());
                if let Some(language) = code.language() {
                    facts = facts.language(language);
                }
                let child_context = context
                    .with_role(TextRole::CodeBlock)
                    .with_language(code.language());
                stamp_text(
                    self.render_literal(code.body(), &child_context).no_wrap(),
                    facts,
                )
            }
            BlockKind::Table(table) => self.render_table(block, table, &context),
            BlockKind::ThematicBreak => stamp_text(
                View::text("───").no_wrap(),
                semantic_view_facts(&context, TextRole::ThematicBreak, block.annotations())
                    .part(TextPart::ThematicRule),
            ),
            BlockKind::RawBlock { format, body } => {
                let facts = semantic_view_facts(&context, TextRole::RawBlock, block.annotations())
                    .format(format);
                let child_context = context.with_role(TextRole::RawBlock).with_format(format);
                stamp_text(self.render_literal(body, &child_context).no_wrap(), facts)
            }
            BlockKind::Container { blocks } => {
                let facts = semantic_view_facts(&context, TextRole::Container, block.annotations());
                let child_context = context.with_role(TextRole::Container);
                stamp_view(self.render_blocks(blocks, &child_context), facts)
            }
        }
    }

    pub(super) fn render_blocks(&self, blocks: &[Block], context: &RenderContext) -> View {
        View::vertical(|column| {
            column.gap(self.policy.block_gap());
            for block in blocks {
                column.child(self.lower_block(block, context));
            }
        })
    }

    pub(super) fn render_paragraph(
        &self,
        block: &Block,
        content: &InlineContent,
        context: &RenderContext,
        alignment: Option<HorizontalAlign>,
    ) -> View {
        let facts = semantic_view_facts(context, TextRole::Paragraph, block.annotations());
        let mut text = self.render_inline_content(content, context);
        if let Some(alignment) = alignment {
            text = text.text_align(alignment);
        }
        stamp_text(text, facts)
    }

    fn render_heading(
        &self,
        block: &Block,
        level: HeadingLevel,
        content: &InlineContent,
        context: &RenderContext,
    ) -> View {
        let facts = semantic_view_facts(context, TextRole::Heading, block.annotations())
            .heading_level(level);
        stamp_text(self.render_inline_content(content, context), facts)
    }

    fn render_quote(&self, block: &Block, blocks: &[Block], context: &RenderContext) -> View {
        let facts = semantic_view_facts(context, TextRole::BlockQuote, block.annotations());
        let marker_facts = part_facts(context, TextPart::QuoteMarker, block.annotations());
        let prefix = stamp_text(View::text("> "), marker_facts.clone());
        let continuation = stamp_text(View::text("> "), marker_facts);
        let body = self.render_blocks(blocks, &context.with_role(TextRole::BlockQuote));
        stamp_view(View::hanging(prefix, continuation, body), facts)
    }

    fn render_list(&self, block: &Block, list: &List, context: &RenderContext) -> View {
        let kind = match list.marker() {
            ListMarker::Bullet => TextListKind::Bullet,
            ListMarker::Ordered { .. } => TextListKind::Ordered,
        };
        let facts =
            semantic_view_facts(context, TextRole::List, block.annotations()).list_kind(kind);
        let item_context = context.with_role(TextRole::List).with_list_kind(kind);
        let items = View::vertical(|column| {
            column.gap(if list.tight() {
                0
            } else {
                self.policy.block_gap()
            });
            for (index, item) in list.items().iter().enumerate() {
                column.child(self.render_list_item(list.marker(), index, item, &item_context));
            }
        });
        stamp_view(items, facts)
    }

    fn render_list_item(
        &self,
        marker: ListMarker,
        index: usize,
        item: &ListItem,
        context: &RenderContext,
    ) -> View {
        let item_context = context.for_node(item.annotations());
        let task_state = item.checked().map(|checked| {
            if checked {
                TextTaskState::Checked
            } else {
                TextTaskState::Unchecked
            }
        });
        let mut facts = semantic_view_facts(&item_context, TextRole::ListItem, item.annotations());
        if let Some(state) = task_state {
            facts = facts.task_state(state);
        }
        let marker_text = list_marker_text(marker, index, item.checked());
        let continuation = " ".repeat(marker_text.chars().count());
        let part = if task_state.is_some() {
            TextPart::TaskMarker
        } else {
            TextPart::ListMarker
        };
        let mut marker_context = item_context.clone();
        marker_context.task_state = task_state;
        let prefix = stamp_text(
            View::text(marker_text),
            part_facts(&marker_context, part, item.annotations()),
        );
        let body = self.render_blocks(
            item.blocks(),
            &item_context
                .with_role(TextRole::ListItem)
                .with_task_state(task_state),
        );
        stamp_view(View::hanging(prefix, continuation, body), facts)
    }

    fn render_table(&self, block: &Block, table: &Table, context: &RenderContext) -> View {
        let facts = semantic_view_facts(context, TextRole::Table, block.annotations());
        let table_context = context.with_role(TextRole::Table);
        let body = View::vertical(|column| {
            column.gap(self.policy.block_gap());
            if let Some(caption) = table.caption() {
                column.child(self.render_blocks(caption, &table_context));
            }
            for (row_index, row) in table.rows().iter().enumerate() {
                let section = if row_index < table.header_rows() {
                    TextTableSection::Header
                } else {
                    TextTableSection::Body
                };
                let row_context = table_context
                    .for_node(row.annotations())
                    .with_table_section(section);
                let row_facts =
                    semantic_view_facts(&row_context, TextRole::TableRow, row.annotations());
                let row_view = View::horizontal(|horizontal| {
                    for (column_index, cell) in row.cells().iter().enumerate() {
                        horizontal.flex(self.render_cell(
                            cell,
                            table,
                            column_index,
                            &row_context.with_role(TextRole::TableRow),
                        ));
                    }
                });
                column.child(stamp_view(row_view, row_facts));
            }
        });
        stamp_view(body, facts)
    }

    fn render_cell(
        &self,
        cell: &TableCell,
        table: &Table,
        column_index: usize,
        context: &RenderContext,
    ) -> View {
        let context = context.for_node(cell.annotations());
        let facts = semantic_view_facts(&context, TextRole::TableCell, cell.annotations());
        let child_context = context.with_role(TextRole::TableCell);
        let inner = if cell.blocks().len() == 1 {
            match cell.blocks()[0].kind() {
                BlockKind::Paragraph(content) => {
                    let alignment = cell.alignment().or_else(|| {
                        table
                            .columns()
                            .get(column_index)
                            .map(|column| column.alignment())
                    });
                    self.render_paragraph(
                        &cell.blocks()[0],
                        content,
                        &child_context,
                        alignment.map(to_horizontal_align),
                    )
                }
                _ => self.render_blocks(cell.blocks(), &child_context),
            }
        } else {
            self.render_blocks(cell.blocks(), &child_context)
        };
        stamp_view(inner.container(), facts)
    }
}

fn list_marker_text(marker: ListMarker, index: usize, checked: Option<bool>) -> String {
    let marker = match marker {
        ListMarker::Bullet => "- ".to_owned(),
        ListMarker::Ordered {
            start,
            style,
            delimiter,
        } => format_marker(start.saturating_add(index as u64), style, delimiter),
    };
    match checked {
        Some(true) => format!("[x] {marker}"),
        Some(false) => format!("[ ] {marker}"),
        None => marker,
    }
}

fn format_marker(value: u64, style: NumberStyle, delimiter: NumberDelimiter) -> String {
    let number = format_number(value, style);
    match delimiter {
        NumberDelimiter::Period => format!("{number}. "),
        NumberDelimiter::Paren => format!("{number}) "),
        NumberDelimiter::TwoParens => format!("({number}) "),
    }
}

fn format_number(value: u64, style: NumberStyle) -> String {
    match style {
        NumberStyle::Decimal => value.to_string(),
        NumberStyle::LowerAlpha => alpha_number(value, b'a'),
        NumberStyle::UpperAlpha => alpha_number(value, b'A'),
        NumberStyle::LowerRoman => roman_number(value).to_lowercase(),
        NumberStyle::UpperRoman => roman_number(value),
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

fn to_horizontal_align(alignment: Alignment) -> HorizontalAlign {
    match alignment {
        Alignment::Default | Alignment::Start => HorizontalAlign::Start,
        Alignment::Center => HorizontalAlign::Center,
        Alignment::End => HorizontalAlign::End,
    }
}
