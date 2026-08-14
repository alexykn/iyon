//! Promote delimiter-less pipe rows into table IR.
//!
//! GFM tables need a `| --- |` row. Models often omit it, so pulldown leaves a
//! paragraph of `|` lines. Those never become a Grid and cannot share tracks.

use iyon_tui::stream::{StreamOffset, StreamRange};
use iyon_tui::text::{
    Alignment, Block, BlockKind, InlineContent, InlineKind, Table, TableCell, TableColumn,
    TableRow, TextIrError, TextProvenance, TextRewriter, TextRun, walk_rewrite_block,
};

pub(super) struct PipeTableRewriter;

impl TextRewriter for PipeTableRewriter {
    type Error = TextIrError;

    fn rewrite_block(&mut self, block: Block) -> Result<Block, Self::Error> {
        if let BlockKind::Paragraph(content) = block.kind()
            && let Some(table) = table_from_pipe_paragraph(content)
        {
            return Ok(Block::table(table).with_annotations(block.annotations().clone()));
        }
        walk_rewrite_block(self, block)
    }
}

fn table_from_pipe_paragraph(content: &InlineContent) -> Option<Table> {
    let lines = pipe_lines(content)?;
    let rows: Vec<Vec<String>> = lines
        .iter()
        .map(|line| split_cells(line))
        .collect::<Option<_>>()?;
    let width = rows.first()?.len();
    if width < 2 || rows.iter().any(|row| row.len() != width) {
        return None;
    }

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
    for inline in content.iter() {
        match inline.kind() {
            InlineKind::Break(_) => lines.push(String::new()),
            InlineKind::Text(run) => lines.last_mut()?.push_str(run.text()),
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
    let cells: Vec<String> = line[1..line.len() - 1]
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect();
    (cells.len() >= 2).then_some(cells)
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
