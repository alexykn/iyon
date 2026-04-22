use ratatui::text::Line;

use crate::{
    core::input::InputBuffer,
    util::format::{TimelineItem, TuiFormatter},
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

    pub(crate) fn sync_chat_from_transcript(&mut self, width: u16) {
        self.transcript.ensure_render_cache(width);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TranscriptState {
    pub(crate) canonical_items: Vec<TimelineItem>,
    pub(crate) rendered_rows_cache: Option<TranscriptRenderCache>,
    pub(crate) committed_rows: usize,
    formatter: TuiFormatter,
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            canonical_items: Vec::new(),
            rendered_rows_cache: None,
            committed_rows: 0,
            formatter: TuiFormatter,
        }
    }
}

impl TranscriptState {
    pub(crate) fn push_item(&mut self, item: TimelineItem) {
        let formatted_rows = if self.rendered_rows_cache.is_some() {
            Some(self.formatter.format(&item))
        } else {
            None
        };

        self.canonical_items.push(item);

        if let (Some(cache), Some(mut rows)) = (self.rendered_rows_cache.as_mut(), formatted_rows) {
            if !cache.rows.is_empty() {
                cache.rows.push(Line::from(""));
            }
            cache.rows.append(&mut rows);
            self.committed_rows = self.committed_rows.min(cache.rows.len());
        }
    }

    pub(crate) fn ensure_render_cache(&mut self, width: u16) {
        let should_recompute = match self.rendered_rows_cache.as_ref() {
            Some(cache) => cache.width != width,
            None => true,
        };

        if !should_recompute {
            return;
        }

        let rows = self.rows_from_canonical();
        self.rendered_rows_cache = Some(TranscriptRenderCache { width, rows });

        if let Some(cache) = self.rendered_rows_cache.as_ref() {
            self.committed_rows = self.committed_rows.min(cache.rows.len());
        } else {
            self.committed_rows = 0;
        }
    }

    pub(crate) fn uncommitted_rows(&self) -> &[Line<'static>] {
        let Some(cache) = self.rendered_rows_cache.as_ref() else {
            return &[];
        };

        let start = self.committed_rows.min(cache.rows.len());
        &cache.rows[start..]
    }

    pub(crate) fn uncommitted_len(&self) -> usize {
        self.uncommitted_rows().len()
    }

    pub(crate) fn mark_rows_committed(&mut self, rows: usize) {
        self.committed_rows = self.committed_rows.saturating_add(rows);
        if let Some(cache) = self.rendered_rows_cache.as_ref() {
            self.committed_rows = self.committed_rows.min(cache.rows.len());
        }
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
}

#[derive(Debug, Clone)]
pub(crate) struct ActivePaneState {
    pub(crate) kind: ActivePaneKind,
    pub(crate) behavior: ActiveBehavior,
    pub(crate) spill_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivePaneKind {
    WorkingSpinner,
    AgentStreaming,
    SlashMenu,
    FilePicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveBehavior {
    SpillToTranscript,
    OccludeOnly,
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
