use anyhow::Result;
use ratatui::text::Line;

use crate::transcript::TranscriptState;
use crate::{scrollback::committer::ScrollbackCommitter, terminal::InlineTerminal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlushResult {
    Done,
    More,
}

#[derive(Debug, Default)]
pub(crate) struct ScrollbackCoordinator {
    committer: ScrollbackCommitter,
}

impl ScrollbackCoordinator {
    pub(crate) fn commit_rows_checked(
        &mut self,
        terminal: &mut InlineTerminal,
        rows: &[Line<'static>],
    ) -> Result<()> {
        const MAX_ROWS_PER_COMMIT: usize = u16::MAX as usize;

        for chunk in rows.chunks(MAX_ROWS_PER_COMMIT) {
            let result = self.committer.commit_rows(terminal, chunk)?;
            if let Some(diagnostics) = result.diagnostics {
                return Err(anyhow::anyhow!(
                    "scrollback commit truncated: inserted {} of {} rows (truncated {})",
                    result.rows_inserted,
                    diagnostics.requested_rows,
                    diagnostics.truncated_rows
                ));
            }
            if result.rows_inserted != chunk.len() {
                return Err(anyhow::anyhow!(
                    "scrollback commit mismatch: inserted {} of {} rows",
                    result.rows_inserted,
                    chunk.len()
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn commit_running_overflow(
        &mut self,
        terminal: &mut InlineTerminal,
        transcript: &mut TranscriptState,
        commit_capacity_rows: usize,
    ) -> Result<()> {
        let uncommitted_len = transcript.uncommitted_len();
        if commit_capacity_rows == 0 || uncommitted_len <= commit_capacity_rows {
            return Ok(());
        }

        let overflow = uncommitted_len - commit_capacity_rows;
        let rows = transcript.uncommitted_rows()[..overflow].to_vec();
        self.commit_rows_checked(terminal, &rows)?;
        transcript.mark_rows_committed(rows.len());
        Ok(())
    }

    pub(crate) fn flush_history_chunk(
        &mut self,
        terminal: &mut InlineTerminal,
        transcript: &mut TranscriptState,
        max_rows: usize,
    ) -> Result<FlushResult> {
        let uncommitted_len = transcript.uncommitted_len();
        if uncommitted_len == 0 {
            return Ok(FlushResult::Done);
        }

        let chunk_len = uncommitted_len.min(max_rows);
        let rows = transcript.uncommitted_rows()[..chunk_len].to_vec();
        self.commit_rows_checked(terminal, &rows)?;
        transcript.mark_rows_committed(chunk_len);
        terminal.invalidate_next_draw();

        if transcript.uncommitted_len() == 0 {
            Ok(FlushResult::Done)
        } else {
            Ok(FlushResult::More)
        }
    }
}
