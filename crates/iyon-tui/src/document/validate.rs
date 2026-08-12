use crate::StreamRange;

use super::{TextContent, TextProjectionError, TextProvenance, TextRun};

/// Validates a standalone generic text content value within a source span.
pub fn validate_text_content(
    content: &TextContent,
    owner: StreamRange,
) -> Result<(), TextProjectionError> {
    match content {
        TextContent::Raw(raw) => {
            if raw.len() as u64 != owner.len() {
                return Err(TextProjectionError::RawByteLengthMismatch {
                    source: owner,
                    text_len: raw.len() as u64,
                });
            }
        }
        TextContent::Block(block) => validate_block(block, owner)?,
    }
    Ok(())
}

/// Validates P1 projection contracts plus generic text IR provenance rules.
///
/// Exact provenance is checked for structural plausibility, UTF-8-compatible
/// length, and containment. This function has no root source witness and
/// therefore cannot prove that Exact display bytes equal the source bytes.
pub fn validate_text_projection(
    projection: &crate::Projection<TextContent>,
) -> Result<(), TextProjectionError> {
    crate::projection::validate_projection(projection).map_err(TextProjectionError::Projection)?;
    for span in projection.spans() {
        let owner = span.source();
        let raw_count = span
            .values()
            .iter()
            .filter(|value| matches!(value, TextContent::Raw(_)))
            .count();
        if raw_count > 0 {
            if raw_count != 1 || span.values().len() != 1 {
                return Err(TextProjectionError::RawMustBeSoleValue { source: owner });
            }
            let TextContent::Raw(raw) = &span.values()[0] else {
                unreachable!()
            };
            if raw.len() as u64 != owner.len() {
                return Err(TextProjectionError::RawByteLengthMismatch {
                    source: owner,
                    text_len: raw.len() as u64,
                });
            }
        }
        for value in span.values() {
            if let TextContent::Block(block) = value {
                validate_block(block, owner)?;
            }
        }
    }
    Ok(())
}

fn validate_block(block: &super::Block, owner: StreamRange) -> Result<(), TextProjectionError> {
    match block.kind() {
        super::BlockKind::Paragraph(content) | super::BlockKind::Heading { content, .. } => {
            for inline in content.iter() {
                validate_inline(inline, owner)?;
            }
        }
        super::BlockKind::BlockQuote { blocks } | super::BlockKind::Container { blocks } => {
            for block in blocks.iter() {
                validate_block(block, owner)?;
            }
        }
        super::BlockKind::List(list) => {
            for item in list.items() {
                for block in item.blocks() {
                    validate_block(block, owner)?;
                }
            }
        }
        super::BlockKind::CodeBlock(code) => validate_literal(code.body(), owner)?,
        super::BlockKind::Table(table) => {
            table.validate().map_err(TextProjectionError::Ir)?;
            if let Some(caption) = table.caption() {
                for block in caption {
                    validate_block(block, owner)?;
                }
            }
            for row in table.rows() {
                for cell in row.cells() {
                    for block in cell.blocks() {
                        validate_block(block, owner)?;
                    }
                }
            }
        }
        super::BlockKind::ThematicBreak => {}
        super::BlockKind::RawBlock { body, .. } => validate_literal(body, owner)?,
    }
    Ok(())
}

fn validate_inline(inline: &super::Inline, owner: StreamRange) -> Result<(), TextProjectionError> {
    match inline.kind() {
        super::InlineKind::Text(run) => validate_run(run, owner),
        super::InlineKind::Image(image) => {
            for child in image.alt().iter() {
                validate_inline(child, owner)?;
            }
            Ok(())
        }
        super::InlineKind::RawInline { body, .. } => validate_literal(body, owner),
        super::InlineKind::Break(_) => Ok(()),
    }
}

fn validate_literal(
    literal: &super::LiteralText,
    owner: StreamRange,
) -> Result<(), TextProjectionError> {
    for run in literal.runs() {
        validate_run(run, owner)?;
    }
    Ok(())
}

fn validate_run(run: &TextRun, owner: StreamRange) -> Result<(), TextProjectionError> {
    let Some(range) = (match run.provenance() {
        TextProvenance::Exact(range) | TextProvenance::Derived(range) => Some(*range),
        TextProvenance::Synthetic => None,
    }) else {
        return Ok(());
    };
    if range.start() < owner.start() || range.end() > owner.end() {
        return Err(TextProjectionError::NestedRangeOutsideSpan { owner, range });
    }
    if matches!(run.provenance(), TextProvenance::Exact(_))
        && (range.start().as_u64() < owner.start().as_u64()
            || range.end().as_u64() > owner.end().as_u64())
    {
        return Err(TextProjectionError::NestedRangeOutsideSpan { owner, range });
    }
    if matches!(run.provenance(), TextProvenance::Exact(_))
        && run.text().len() as u64 != range.len()
    {
        return Err(TextProjectionError::ExactLengthMismatch {
            range,
            text_len: run.text().len() as u64,
        });
    }
    Ok(())
}
