use ratatui::text::Line;

use crate::{
    core::{active::ActivePaneState, input::InputBuffer},
    util::format::{TimelineItem, TuiFormatter},
    util::wrapping::{TranscriptCommitBoundary, wrap_transcript_rows},
};

#[derive(Debug, Default)]
pub(crate) struct AppState {
    pub(crate) input: InputBuffer,
    pub(crate) transcript: TranscriptState,
    pub(crate) active: Option<ActivePaneState>,

    pub(crate) info: InfoState,
    pub(crate) input_content_width: u16,
    pub(crate) scroll: u16,

    pub(crate) exit_state: ExitState,
}

impl AppState {
    pub(crate) fn append_timeline_item(&mut self, item: TimelineItem) {
        self.transcript.push_item(item);
    }

    pub(crate) fn append_agent_message(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.transcript
            .push_item(TimelineItem::AgentMessage { text });
    }

    pub(crate) fn request_exit(&mut self) {
        if matches!(self.exit_state, ExitState::Running) {
            self.exit_state = ExitState::Requested;
        }
    }

    pub(crate) fn start_working_pane(&mut self) {
        if self.active.is_none() {
            self.active = Some(ActivePaneState::working_spinner());
        }
    }

    pub(crate) fn receive_assistant_delta(&mut self, chunk: &str) {
        if self.active.is_none() {
            self.start_working_pane();
        }

        if let Some(active) = self.active.as_mut() {
            active.push_assistant_delta(chunk);
        }
    }

    pub(crate) fn tick_active(&mut self) {
        if let Some(active) = self.active.as_mut() {
            active.tick();
        }
    }

    pub(crate) fn take_active_unfrozen_transcript_text(&mut self) -> Option<String> {
        self.active
            .take()
            .and_then(ActivePaneState::into_unfrozen_transcript_text)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TranscriptState {
    pub(crate) canonical_items: Vec<TimelineItem>,
    pub(crate) rendered_rows_cache: Option<TranscriptRenderCache>,
    pub(crate) commit_boundary: TranscriptCommitBoundary,
    formatter: TuiFormatter,
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            canonical_items: Vec::new(),
            rendered_rows_cache: None,
            commit_boundary: TranscriptCommitBoundary::default(),
            formatter: TuiFormatter,
        }
    }
}

impl TranscriptState {
    pub(crate) fn push_item(&mut self, item: TimelineItem) {
        self.canonical_items.push(item);
        self.rendered_rows_cache = None;
    }

    pub(crate) fn ensure_render_cache(&mut self, width: u16) {
        let should_recompute = match self.rendered_rows_cache.as_ref() {
            Some(cache) => cache.width != width,
            None => true,
        };

        if !should_recompute {
            return;
        }

        let logical_rows = self.rows_from_canonical();
        let cache = TranscriptRenderCache::from_logical_rows(
            width.max(1),
            &logical_rows,
            self.commit_boundary,
        );
        self.rendered_rows_cache = Some(cache);
    }

    pub(crate) fn uncommitted_rows(&self) -> &[Line<'static>] {
        let Some(cache) = self.rendered_rows_cache.as_ref() else {
            return &[];
        };

        &cache.rows
    }

    pub(crate) fn uncommitted_len(&self) -> usize {
        self.uncommitted_rows().len()
    }

    pub(crate) fn mark_rows_committed(&mut self, rows: usize) {
        let Some(cache) = self.rendered_rows_cache.as_mut() else {
            return;
        };

        let committed_rows = rows.min(cache.rows.len());
        if committed_rows == 0 {
            return;
        }

        self.commit_boundary = cache.row_end_boundaries[committed_rows - 1];
        cache.rows.drain(..committed_rows);
        cache.row_end_boundaries.drain(..committed_rows);
    }

    fn rows_from_canonical(&self) -> Vec<Line<'static>> {
        let mut rows = Vec::new();

        for item in &self.canonical_items {
            if !rows.is_empty() {
                rows.push(Line::from(""));
            }
            rows.extend(self.formatter.format(item));
        }

        rows
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TranscriptRenderCache {
    pub(crate) width: u16,
    pub(crate) rows: Vec<Line<'static>>,
    row_end_boundaries: Vec<TranscriptCommitBoundary>,
}

impl TranscriptRenderCache {
    fn from_logical_rows(
        width: u16,
        logical_rows: &[Line<'static>],
        commit_boundary: TranscriptCommitBoundary,
    ) -> Self {
        let wrapped = wrap_transcript_rows(width, logical_rows, commit_boundary);

        Self {
            width,
            rows: wrapped.rows,
            row_end_boundaries: wrapped.row_end_boundaries,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExitState {
    #[default]
    Running,
    Requested,
    FinalizeActive,
    FlushHistory,
    FinalFrame,
    Done,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InfoState {
    pub(crate) status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn wraps_transcript_rows_to_visual_width() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AgentMessage {
            text: "abcdef".to_string(),
        });

        transcript.ensure_render_cache(3);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["", "abc", "def", ""]);
    }

    #[test]
    fn commit_boundary_survives_resize_after_partial_line_commit() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AgentMessage {
            text: "abcdef".to_string(),
        });

        transcript.ensure_render_cache(3);
        transcript.mark_rows_committed(2);
        transcript.ensure_render_cache(2);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["de", "f", ""]);
    }
}
