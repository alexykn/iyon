//! INTERNAL PRESENTATION MECHANICS.
//!
//! Owns loss-aware physical commit into native terminal history. Feature code
//! must not depend on this coordinator.

use anyhow::Result;
use ratatui::text::Line;

use crate::presentation::{HostedStream, PreparedStreamFrame, StreamingContent};
use crate::transcript::TranscriptState;
use crate::{
    scrollback::committer::ScrollbackCommitter,
    terminal::{InlineTerminal, ratatui::row_to_line},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlushResult {
    Done,
    More,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommitRowsOutcome {
    inserted: usize,
    requested: usize,
}

impl CommitRowsOutcome {
    fn is_blocked(self) -> bool {
        self.requested != 0 && self.inserted == 0
    }
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
        let outcome = self.commit_rows_lossless(terminal, rows)?;
        if outcome.inserted != outcome.requested {
            return Err(anyhow::anyhow!(
                "scrollback commit incomplete: inserted {} of {} rows",
                outcome.inserted,
                outcome.requested
            ));
        }
        Ok(())
    }

    fn commit_rows_lossless(
        &mut self,
        terminal: &mut InlineTerminal,
        rows: &[Line<'static>],
    ) -> Result<CommitRowsOutcome> {
        const MAX_ROWS_PER_COMMIT: usize = u16::MAX as usize;

        let mut inserted = 0usize;
        for chunk in rows.chunks(MAX_ROWS_PER_COMMIT) {
            let result = self.committer.commit_rows(terminal, chunk)?;
            inserted = inserted.saturating_add(result.rows_inserted);

            // A partial insert means the terminal accepted only part of the requested
            // scrollback mutation, usually because the viewport changed under us. Stop
            // here and let the caller mark exactly the rows that actually made it into
            // history; never advance TranscriptState past what the terminal accepted.
            if result.rows_inserted != chunk.len() || result.diagnostics.is_some() {
                break;
            }
        }

        Ok(CommitRowsOutcome {
            inserted,
            requested: rows.len(),
        })
    }

    pub(crate) fn commit_stream_frame<C: StreamingContent>(
        &mut self,
        terminal: &mut InlineTerminal,
        hosted: &mut HostedStream<C>,
        prepared: PreparedStreamFrame,
    ) -> Result<()> {
        let rows = prepared
            .history
            .rows
            .as_slice()
            .iter()
            .map(row_to_line)
            .collect::<Vec<_>>();
        self.commit_rows_checked(terminal, &rows)?;
        hosted.apply_commit_success(prepared.history);
        Ok(())
    }

    /// Advances the currently committable transcript prefix for ordering purposes.
    ///
    /// This is driven by the single global conversation overflow budget. The
    /// caller supplies only rows still eligible at the current history head.
    pub(crate) fn commit_transcript_prefix(
        &mut self,
        terminal: &mut InlineTerminal,
        transcript: &mut TranscriptState,
        max_rows: usize,
    ) -> Result<usize> {
        let rows_to_commit = max_rows
            .min(transcript.uncommitted_len())
            .min(transcript.committable_len());
        if rows_to_commit == 0 {
            return Ok(0);
        }

        let rows = transcript.uncommitted_rows()[..rows_to_commit].to_vec();
        let outcome = self.commit_rows_lossless(terminal, &rows)?;
        transcript.mark_rows_committed(outcome.inserted);
        Ok(outcome.inserted)
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

        let chunk_len = uncommitted_len
            .min(max_rows)
            .min(transcript.committable_len());
        if chunk_len == 0 {
            return Ok(FlushResult::Blocked);
        }
        let rows = transcript.uncommitted_rows()[..chunk_len].to_vec();
        let outcome = self.commit_rows_lossless(terminal, &rows)?;
        transcript.mark_rows_committed(outcome.inserted);
        terminal.invalidate_next_draw();

        if transcript.uncommitted_len() == 0 {
            Ok(FlushResult::Done)
        } else if outcome.is_blocked() {
            Ok(FlushResult::Blocked)
        } else {
            Ok(FlushResult::More)
        }
    }
}
