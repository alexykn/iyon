use std::sync::Arc;

use super::{Block, BlockKind, Inline, InlineContent, InlineKind, LiteralText, TextContent};

/// Read-only traversal over all generic text IR descendants.
pub trait TextVisitor {
    fn visit_content(&mut self, content: &TextContent) {
        walk_content(self, content);
    }
    fn visit_block(&mut self, block: &Block) {
        walk_block(self, block);
    }
    fn visit_inline(&mut self, inline: &Inline) {
        walk_inline(self, inline);
    }
    fn visit_literal(&mut self, literal: &LiteralText) {
        walk_literal(self, literal);
    }
}

pub fn walk_content<V: TextVisitor + ?Sized>(visitor: &mut V, content: &TextContent) {
    if let TextContent::Block(block) = content {
        visitor.visit_block(block);
    }
}

pub fn walk_block<V: TextVisitor + ?Sized>(visitor: &mut V, block: &Block) {
    match block.kind() {
        BlockKind::Paragraph(content) | BlockKind::Heading { content, .. } => {
            walk_inline_content(visitor, content)
        }
        BlockKind::BlockQuote { blocks } | BlockKind::Container { blocks } => {
            for block in blocks.iter() {
                visitor.visit_block(block);
            }
        }
        BlockKind::List(list) => {
            for item in list.items() {
                for block in item.blocks() {
                    visitor.visit_block(block);
                }
            }
        }
        BlockKind::CodeBlock(code) => visitor.visit_literal(code.body()),
        BlockKind::Table(table) => {
            if let Some(caption) = table.caption() {
                for block in caption {
                    visitor.visit_block(block);
                }
            }
            for row in table.rows() {
                for cell in row.cells() {
                    for block in cell.blocks() {
                        visitor.visit_block(block);
                    }
                }
            }
        }
        BlockKind::ThematicBreak => {}
        BlockKind::RawBlock { body, .. } => visitor.visit_literal(body),
    }
}

pub fn walk_inline_content<V: TextVisitor + ?Sized>(visitor: &mut V, content: &InlineContent) {
    for inline in content.iter() {
        visitor.visit_inline(inline);
    }
}

pub fn walk_inline<V: TextVisitor + ?Sized>(visitor: &mut V, inline: &Inline) {
    match inline.kind() {
        InlineKind::Text(_) | InlineKind::Break(_) => {}
        InlineKind::Image(image) => walk_inline_content(visitor, image.alt()),
        InlineKind::RawInline { body, .. } => visitor.visit_literal(body),
    }
}

pub fn walk_literal<V: TextVisitor + ?Sized>(_visitor: &mut V, _literal: &LiteralText) {}

/// Persistent recursive rewriting with identity preservation for unchanged paths.
pub trait TextRewriter {
    type Error;

    fn rewrite_content(&mut self, content: TextContent) -> Result<TextContent, Self::Error> {
        walk_rewrite_content(self, content)
    }
    fn rewrite_block(&mut self, block: Block) -> Result<Block, Self::Error> {
        walk_rewrite_block(self, block)
    }
    fn rewrite_inline(&mut self, inline: Inline) -> Result<Inline, Self::Error> {
        walk_rewrite_inline(self, inline)
    }
    fn rewrite_literal(&mut self, literal: LiteralText) -> Result<LiteralText, Self::Error> {
        walk_rewrite_literal(self, literal)
    }
}

pub fn walk_rewrite_content<R: TextRewriter + ?Sized>(
    rewriter: &mut R,
    content: TextContent,
) -> Result<TextContent, R::Error> {
    match content {
        TextContent::Raw(_) => Ok(content),
        TextContent::Block(block) => rewriter.rewrite_block(block).map(TextContent::Block),
    }
}

pub fn walk_rewrite_block<R: TextRewriter + ?Sized>(
    rewriter: &mut R,
    block: Block,
) -> Result<Block, R::Error> {
    let annotations = block.annotations().clone();
    let kind = match block.kind().clone() {
        BlockKind::Paragraph(content) => {
            BlockKind::Paragraph(rewrite_inline_content(rewriter, content)?)
        }
        BlockKind::Heading { level, content } => BlockKind::Heading {
            level,
            content: rewrite_inline_content(rewriter, content)?,
        },
        BlockKind::BlockQuote { blocks } => BlockKind::BlockQuote {
            blocks: rewrite_blocks(rewriter, &blocks)?,
        },
        BlockKind::Container { blocks } => BlockKind::Container {
            blocks: rewrite_blocks(rewriter, &blocks)?,
        },
        BlockKind::List(mut list) => {
            let mut items = Vec::with_capacity(list.items().len());
            for item in list.items() {
                let blocks = rewrite_blocks(rewriter, item.blocks().into())?;
                items.push(
                    super::ListItem::new(blocks.iter().cloned())
                        .with_annotations(item.annotations().clone())
                        .with_checked(item.checked()),
                );
            }
            list = super::List::new(list.marker(), list.tight(), items);
            BlockKind::List(list)
        }
        BlockKind::CodeBlock(code) => BlockKind::CodeBlock(super::CodeBlock::new(
            code.language().cloned(),
            code.info(),
            rewriter.rewrite_literal(code.body().clone())?,
        )),
        BlockKind::Table(table) => BlockKind::Table(rewrite_table(rewriter, table)?),
        BlockKind::ThematicBreak => BlockKind::ThematicBreak,
        BlockKind::RawBlock { format, body } => BlockKind::RawBlock {
            format,
            body: rewriter.rewrite_literal(body)?,
        },
    };
    let rebuilt = Block::from_parts(kind, annotations);
    Ok(
        if rebuilt.kind() == block.kind() && rebuilt.annotations() == block.annotations() {
            block
        } else {
            rebuilt
        },
    )
}

fn rewrite_blocks<R: TextRewriter + ?Sized>(
    rewriter: &mut R,
    blocks: &[Block],
) -> Result<Arc<[Block]>, R::Error> {
    let mut changed = false;
    let mut output = Vec::with_capacity(blocks.len());
    for block in blocks {
        let rewritten = rewriter.rewrite_block(block.clone())?;
        changed |= !rewritten.ptr_eq(block);
        output.push(rewritten);
    }
    Ok(if changed {
        output.into()
    } else {
        blocks.to_vec().into()
    })
}

fn rewrite_inline_content<R: TextRewriter + ?Sized>(
    rewriter: &mut R,
    content: InlineContent,
) -> Result<InlineContent, R::Error> {
    let mut changed = false;
    let mut items = Vec::with_capacity(content.len());
    for inline in content.items() {
        let rewritten = rewriter.rewrite_inline(inline.clone())?;
        changed |= rewritten != *inline;
        items.push(rewritten);
    }
    Ok(if changed {
        InlineContent::new(items)
    } else {
        content
    })
}

fn rewrite_table<R: TextRewriter + ?Sized>(
    rewriter: &mut R,
    table: super::Table,
) -> Result<super::Table, R::Error> {
    let caption = table.caption().map(|blocks| blocks.to_vec());
    let mut rows = Vec::with_capacity(table.rows().len());
    for row in table.rows() {
        let mut cells = Vec::with_capacity(row.cells().len());
        for cell in row.cells() {
            cells.push(
                super::TableCell::new(
                    rewrite_blocks(rewriter, cell.blocks())?.iter().cloned(),
                    cell.alignment(),
                    cell.row_span(),
                    cell.col_span(),
                )
                .with_annotations(cell.annotations().clone()),
            );
        }
        rows.push(super::TableRow::new(cells).with_annotations(row.annotations().clone()));
    }
    Ok(super::Table::new(
        caption,
        table.columns().iter().copied(),
        table.header_rows(),
        rows,
    ))
}

pub fn walk_rewrite_inline<R: TextRewriter + ?Sized>(
    rewriter: &mut R,
    inline: Inline,
) -> Result<Inline, R::Error> {
    let marks = inline.marks().clone();
    let annotations = inline.annotations().clone();
    let kind = match inline.kind().clone() {
        InlineKind::Text(_) | InlineKind::Break(_) => inline.kind().clone(),
        InlineKind::Image(mut image) => {
            image = super::Image::new(
                image.destination(),
                image.title(),
                rewrite_inline_content(rewriter, image.alt().clone())?,
            );
            InlineKind::Image(image)
        }
        InlineKind::RawInline { format, body } => InlineKind::RawInline {
            format,
            body: rewriter.rewrite_literal(body)?,
        },
    };
    let rebuilt = Inline::from_parts(kind, marks, annotations);
    Ok(if rebuilt == inline { inline } else { rebuilt })
}

pub fn walk_rewrite_literal<R: TextRewriter + ?Sized>(
    _rewriter: &mut R,
    literal: LiteralText,
) -> Result<LiteralText, R::Error> {
    Ok(literal)
}
