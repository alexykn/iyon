use anyhow::Result;
use ratatui::text::Line;

use crate::terminal::InlineTerminal;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CommitDiagnostics {
    pub(crate) requested_rows: usize,
    pub(crate) truncated_rows: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CommitResult {
    pub(crate) rows_inserted: usize,
    pub(crate) diagnostics: Option<CommitDiagnostics>,
}

#[derive(Debug, Default)]
pub(crate) struct ScrollbackCommitter;

impl ScrollbackCommitter {
    pub(crate) fn commit_rows(
        &mut self,
        terminal: &mut InlineTerminal,
        rows: &[Line<'static>],
    ) -> Result<CommitResult> {
        let rows_inserted = terminal.insert_history_rows(rows)?;
        let truncated_rows = rows.len().saturating_sub(rows_inserted);

        let diagnostics = if truncated_rows > 0 {
            Some(CommitDiagnostics {
                requested_rows: rows.len(),
                truncated_rows,
            })
        } else {
            None
        };

        Ok(CommitResult {
            rows_inserted,
            diagnostics,
        })
    }
}
