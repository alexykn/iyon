//! Promote delimiter-less pipe rows into table IR.
//!
//! GFM tables need a `| --- |` row. Models often omit it, so pulldown leaves a
//! paragraph of `|` lines. Those never become a Grid and cannot share tracks.
//! Open trailing pipe paragraphs stay raw until a following block or seal so
//! live tables do not re-align on every row.

use iyon_tui::projection::{Projection, Projector};
use iyon_tui::stream::{StreamOffset, StreamRange};
use iyon_tui::text::{
    Alignment, Block, BlockKind, InlineContent, InlineKind, List, ListItem, Mark,
    RewriteProjectionError, Table, TableCell, TableColumn, TableRow, TextContent, TextIrError,
    TextProjectionError, TextProvenance, TextRun, validate_text_projection,
};

pub(super) struct PipeTableRewriter;

impl Projector<TextContent> for PipeTableRewriter {
    type Output = TextContent;
    type Error = RewriteProjectionError<TextIrError>;

    fn project(
        &mut self,
        input: &Projection<TextContent>,
    ) -> Result<Projection<Self::Output>, Self::Error> {
        let mut output = input.rebuild();
        let spans = input.spans();
        for (index, span) in spans.iter().enumerate() {
            let following = input.is_sealed()
                || spans[index + 1..]
                    .iter()
                    .any(|later| !later.values().is_empty());
            let values = span
                .values()
                .iter()
                .cloned()
                .enumerate()
                .map(|(value_index, value)| {
                    let closed = following || value_index + 1 < span.values().len();
                    rewrite_content(value, closed).map_err(RewriteProjectionError::Rewrite)
                })
                .collect::<Result<Vec<_>, _>>()?;
            output = output.emit_many(span.source(), values);
        }
        let output = output.finish().map_err(|error| {
            RewriteProjectionError::Invalid(TextProjectionError::Projection(error))
        })?;
        validate_text_projection(&output).map_err(RewriteProjectionError::Invalid)?;
        Ok(output)
    }
}

fn rewrite_content(content: TextContent, closed: bool) -> Result<TextContent, TextIrError> {
    match content {
        TextContent::Raw(_) => Ok(content),
        TextContent::Block(block) => Ok(TextContent::Block(rewrite_block(block, closed)?)),
    }
}

fn rewrite_block(block: Block, closed: bool) -> Result<Block, TextIrError> {
    match block.kind() {
        BlockKind::Paragraph(content) => {
            if closed {
                if let Some(table) = table_from_pipe_paragraph(content) {
                    return Ok(Block::table(table).with_annotations(block.annotations().clone()));
                }
            }
            Ok(block)
        }
        BlockKind::BlockQuote { blocks } => {
            let rewritten = rewrite_blocks(blocks, closed)?;
            if rewritten.as_slice() == blocks.as_ref() {
                return Ok(block);
            }
            Ok(Block::block_quote(rewritten).with_annotations(block.annotations().clone()))
        }
        BlockKind::Container { blocks } => {
            let rewritten = rewrite_blocks(blocks, closed)?;
            if rewritten.as_slice() == blocks.as_ref() {
                return Ok(block);
            }
            Ok(Block::container(rewritten).with_annotations(block.annotations().clone()))
        }
        BlockKind::List(list) => {
            let item_count = list.items().len();
            let mut changed = false;
            let mut items = Vec::with_capacity(item_count);
            for (index, item) in list.items().iter().enumerate() {
                let item_closed = index + 1 < item_count || closed;
                let blocks = rewrite_blocks(item.blocks(), item_closed)?;
                if blocks.as_slice() != item.blocks() {
                    changed = true;
                }
                items.push(
                    ListItem::new(blocks)
                        .with_annotations(item.annotations().clone())
                        .with_checked(item.checked()),
                );
            }
            if !changed {
                return Ok(block);
            }
            Ok(Block::list(List::new(list.marker(), list.tight(), items))
                .with_annotations(block.annotations().clone()))
        }
        _ => Ok(block),
    }
}

fn rewrite_blocks(blocks: &[Block], trailing_closed: bool) -> Result<Vec<Block>, TextIrError> {
    let count = blocks.len();
    blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let closed = index + 1 < count || trailing_closed;
            rewrite_block(block.clone(), closed)
        })
        .collect()
}

fn table_from_pipe_paragraph(content: &InlineContent) -> Option<Table> {
    let lines = pipe_lines(content)?;
    let rows: Vec<Vec<String>> = lines
        .iter()
        .map(|line| split_cells(line))
        .collect::<Option<_>>()?;
    let width = rows.first()?.len();
    if width < 2 {
        return None;
    }
    // First accepted row establishes schema width. Later rows pad/truncate like
    // GFM body rows (Example 204). One malformed LLM row must not reject the
    // whole table. Ambiguous escapes/code pipes stay raw — this is not cmark.
    let rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|mut row| {
            row.truncate(width);
            while row.len() < width {
                row.push(String::new());
            }
            row
        })
        .collect();

    let (header_rows, alignments, body) = if rows.len() >= 3 && is_delimiter_row(&rows[1]) {
        let alignments = rows[1]
            .iter()
            .map(|cell| delimiter_alignment(cell))
            .collect();
        let mut body = vec![rows[0].clone()];
        body.extend(rows[2..].iter().cloned());
        (1usize, alignments, body)
    } else if rows.iter().any(|row| is_delimiter_row(row)) {
        return None;
    } else if rows.len() >= 2 {
        (0usize, vec![Alignment::Start; width], rows)
    } else {
        return None;
    };

    let range = covering_range(content);
    let columns = alignments.into_iter().map(TableColumn::new);
    let table_rows = body.into_iter().map(|cells| {
        TableRow::new(
            cells
                .into_iter()
                .map(|cell| TableCell::text(cell_run(cell, range))),
        )
    });
    Table::new(None::<Vec<Block>>, columns, header_rows, table_rows).ok()
}

fn pipe_lines(content: &InlineContent) -> Option<Vec<String>> {
    let mut lines = vec![String::new()];
    // pulldown maps `\|` to Exact `|` whose range skips the backslash. A
    // same-line source gap before a pipe is that escape; a Break resets the
    // cursor so the `\n` before the next row is not mistaken for one.
    let mut last_exact_end: Option<StreamOffset> = None;
    for inline in content.iter() {
        match inline.kind() {
            InlineKind::Break(_) => {
                last_exact_end = None;
                lines.push(String::new());
            }
            InlineKind::Text(run) => {
                if inline.marks().contains(&Mark::Code) {
                    return None;
                }
                match run.provenance() {
                    TextProvenance::Derived(_) => return None,
                    TextProvenance::Exact(range) => {
                        if last_exact_end.is_some_and(|end| range.start() > end)
                            && run.text().contains('|')
                        {
                            return None;
                        }
                        last_exact_end = Some(range.end());
                    }
                    TextProvenance::Synthetic => last_exact_end = None,
                }
                lines.last_mut()?.push_str(run.text());
            }
            _ => return None,
        }
    }
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if lines.is_empty() || lines.iter().any(|line| line.trim().is_empty()) {
        return None;
    }
    Some(lines)
}

fn split_cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if line.len() < 3 || !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    // Escaped pipes and inline-code pipes are context-sensitive in GFM. This
    // rewriter is a delimiter-less convenience, not a second Markdown parser.
    if line.contains('\\') || line.contains('`') {
        return None;
    }
    let cells: Vec<String> = line[1..line.len() - 1]
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect();
    (!cells.is_empty()).then_some(cells)
}

fn is_delimiter_row(cells: &[String]) -> bool {
    !cells.is_empty() && cells.iter().all(|cell| is_delimiter_cell(cell))
}

fn is_delimiter_cell(cell: &str) -> bool {
    let core = cell.trim_matches(':');
    !core.is_empty() && core.chars().all(|c| c == '-')
}

fn delimiter_alignment(cell: &str) -> Alignment {
    match (cell.starts_with(':'), cell.ends_with(':')) {
        (true, true) => Alignment::Center,
        (false, true) => Alignment::End,
        _ => Alignment::Start,
    }
}

fn covering_range(content: &InlineContent) -> Option<StreamRange> {
    let mut start: Option<StreamOffset> = None;
    let mut end: Option<StreamOffset> = None;
    for inline in content.iter() {
        let Some(run) = inline.as_text() else {
            continue;
        };
        let range = match run.provenance() {
            TextProvenance::Exact(range) | TextProvenance::Derived(range) => *range,
            TextProvenance::Synthetic => continue,
        };
        start = Some(start.map_or(range.start(), |value| value.min(range.start())));
        end = Some(end.map_or(range.end(), |value| value.max(range.end())));
    }
    Some(StreamRange::new(start?, end?))
}

fn cell_run(text: String, range: Option<StreamRange>) -> TextRun {
    match range {
        Some(range) => TextRun::derived(text, range),
        None => TextRun::synthetic(text),
    }
}
