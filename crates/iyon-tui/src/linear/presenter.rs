//! The sole terminal-owning presenter.
//!
//! Everything that touches stdout or moves the terminal cursor goes through
//! `InlinePresenter`. It exposes exactly one terminal-facing operation —
//! [`InlinePresenter::present`] — which appends finalized history rows and draws
//! the live tail + bottom chrome inside a single synchronized transaction.
//!
//! Presentation state advances only after `present` succeeds; a failed write
//! poisons the presenter and no cursor is advanced.

use anyhow::Result;
use ratatui::Frame;

use crate::{
    linear::buffer::SyncUpdateGuard,
    terminal::InlineTerminal,
    view::{BottomLayout, BottomView},
};

/// All rows, layout, and views for one frame. Everything is computed before
/// `present` touches the terminal. The live tail is part of `view`; `history_rows`
/// are the finalized rows being appended to native history.
pub(crate) struct PresentBatch<'a> {
    pub(crate) history_rows: &'a [ratatui::text::Line<'static>],
    pub(crate) layout: BottomLayout,
    pub(crate) view: &'a BottomView<'a>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PresentReceipt {
    pub(crate) history_rows_written: usize,
}

/// Presentation cursor tracking state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PresentedActiveTurn {
    pub(crate) turn_id: u64,
    pub(crate) committed_source_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresenterState {
    Healthy,
    Poisoned,
}

const MAX_INSERT_ROWS: usize = u16::MAX as usize;

pub(crate) struct InlinePresenter {
    pub(crate) terminal: InlineTerminal,
    /// Timeline presentation cursor: index into `TranscriptState::items()` that
    /// has already been written to native history.
    next_timeline_index: usize,
    /// Kind of the last timeline item flushed (for prefix spacing across the
    /// boundary between a flushed slice and the active tail).
    last_flushed_timeline_kind: Option<crate::transcript::TimelineKind>,
    /// Active-turn commit state, by source boundary (width-independent).
    active: Option<PresentedActiveTurn>,
    state: PresenterState,
}

impl InlinePresenter {
    pub(crate) fn new(terminal: InlineTerminal) -> Self {
        Self {
            terminal,
            next_timeline_index: 0,
            last_flushed_timeline_kind: None,
            active: None,
            state: PresenterState::Healthy,
        }
    }

    pub(crate) fn next_timeline_index(&self) -> usize {
        self.next_timeline_index
    }

    pub(crate) fn last_flushed_timeline_kind(&self) -> Option<crate::transcript::TimelineKind> {
        self.last_flushed_timeline_kind
    }

    pub(crate) fn active_committed_source_end(&self, turn_id: u64) -> usize {
        self.active
            .filter(|a| a.turn_id == turn_id)
            .map_or(0, |a| a.committed_source_end)
    }

    /// Begin tracking a new active turn. Rejects if a *different* turn is still
    /// active (a blind counter reset would be incorrect).
    pub(crate) fn begin_active_turn(&mut self, turn_id: u64) -> Result<()> {
        match self.active {
            None => {
                self.active = Some(PresentedActiveTurn {
                    turn_id,
                    committed_source_end: 0,
                });
                Ok(())
            }
            Some(a) if a.turn_id == turn_id => Ok(()),
            Some(a) => anyhow::bail!(
                "cannot begin turn {turn_id}: turn {} is still active",
                a.turn_id
            ),
        }
    }

    /// Present one frame. Appends `history_rows` to native history, then draws
    /// `view` (tail + chrome), all inside one synchronized update. The commit
    /// watermark / timeline cursor advance only on success.
    pub(crate) fn present(&mut self, batch: PresentBatch<'_>) -> Result<PresentReceipt> {
        if self.state == PresenterState::Poisoned {
            anyhow::bail!("terminal presenter is poisoned");
        }

        let result = self.present_inner(batch);
        if result.is_err() {
            self.state = PresenterState::Poisoned;
        }
        result
    }

    fn present_inner(&mut self, batch: PresentBatch<'_>) -> Result<PresentReceipt> {
        // Chunked, append-only history insertion.
        for chunk in batch.history_rows.chunks(MAX_INSERT_ROWS) {
            self.terminal.insert_history_rows(chunk)?;
        }
        let written = batch.history_rows.len();

        let mut sync = SyncUpdateGuard::begin(&mut self.terminal)?;
        let renderer = crate::view::Renderer;
        self.terminal.draw(|frame| {
            render_tail_and_chrome(&renderer, frame, batch.layout, batch.view);
        })?;
        sync.finish()?;

        Ok(PresentReceipt {
            history_rows_written: written,
        })
    }

    /// Advance the timeline cursor and remember the last flushed kind. Call only
    /// after `present` succeeded.
    pub(crate) fn advance_timeline(
        &mut self,
        new_index: usize,
        last_kind: Option<crate::transcript::TimelineKind>,
    ) {
        self.next_timeline_index = self.next_timeline_index.max(new_index);
        if last_kind.is_some() {
            self.last_flushed_timeline_kind = last_kind;
        }
    }

    /// Advance the active-turn source boundary. Call only after `present`
    /// succeeded.
    pub(crate) fn advance_active_cursor(&mut self, turn_id: u64, committed_source_end: usize) {
        if let Some(mut a) = self.active {
            if a.turn_id == turn_id {
                a.committed_source_end = a.committed_source_end.max(committed_source_end);
                self.active = Some(a);
            }
        }
    }

    /// Remove the active turn's presentation state once its content has been
    /// fully committed/finalized.
    pub(crate) fn clear_active_turn(&mut self, turn_id: u64) {
        if self.active.as_ref().is_some_and(|a| a.turn_id == turn_id) {
            self.active = None;
        }
    }

    pub(crate) fn is_poisoned(&self) -> bool {
        self.state == PresenterState::Poisoned
    }

    /// Commit a final batch of rows to native history during shutdown (no live
    /// tail remains). Used after the running loop stops.
    pub(crate) fn commit_final(&mut self, rows: &[ratatui::text::Line<'static>]) -> Result<()> {
        if self.state == PresenterState::Poisoned {
            anyhow::bail!("terminal presenter is poisoned");
        }
        let result = self.commit_final_inner(rows);
        if result.is_err() {
            self.state = PresenterState::Poisoned;
        }
        result
    }

    fn commit_final_inner(&mut self, rows: &[ratatui::text::Line<'static>]) -> Result<()> {
        for chunk in rows.chunks(MAX_INSERT_ROWS) {
            self.terminal.insert_history_rows(chunk)?;
        }
        Ok(())
    }
}

fn render_tail_and_chrome(
    renderer: &crate::view::Renderer,
    frame: &mut Frame,
    layout: BottomLayout,
    view: &BottomView<'_>,
) {
    renderer.draw_bottom(frame, &layout, view);
}
