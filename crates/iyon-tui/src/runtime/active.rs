use std::time::{Duration, Instant};

use ratatui::text::Line;

use crate::transcript::{
    model::TuiFormatter,
    wrap::{TranscriptCommitBoundary, wrap_transcript_rows},
};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ACTIVE_STREAM_UNSTABLE_TAIL_ROWS: usize = 1;

#[derive(Debug, Clone)]
pub(crate) enum ActivePaneState {
    WorkingSpinner {
        stream: ActiveStreamState,
        spinner_frame: usize,
    },
    AssistantStreaming {
        stream: ActiveStreamState,
    },
    SlashMenu,
    FilePicker,
}

impl ActivePaneState {
    pub(crate) fn working_spinner() -> Self {
        Self::WorkingSpinner {
            stream: ActiveStreamState::new(),
            spinner_frame: 0,
        }
    }

    pub(crate) fn kind(&self) -> ActivePaneKind {
        match self {
            Self::WorkingSpinner { .. } => ActivePaneKind::WorkingSpinner,
            Self::AssistantStreaming { .. } => ActivePaneKind::AssistantStreaming,
            Self::SlashMenu => ActivePaneKind::SlashMenu,
            Self::FilePicker => ActivePaneKind::FilePicker,
        }
    }

    pub(crate) fn behavior(&self) -> ActiveBehavior {
        match self {
            Self::WorkingSpinner { .. } | Self::AssistantStreaming { .. } => {
                ActiveBehavior::SpillToTranscript
            }
            Self::SlashMenu | Self::FilePicker => ActiveBehavior::OccludeOnly,
        }
    }

    pub(crate) fn is_spillable(&self) -> bool {
        self.behavior() == ActiveBehavior::SpillToTranscript
    }

    pub(crate) fn stream(&self) -> Option<&ActiveStreamState> {
        match self {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => {
                Some(stream)
            }
            Self::SlashMenu | Self::FilePicker => None,
        }
    }

    pub(crate) fn stream_mut(&mut self) -> Option<&mut ActiveStreamState> {
        match self {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => {
                Some(stream)
            }
            Self::SlashMenu | Self::FilePicker => None,
        }
    }

    pub(crate) fn ensure_stream_render_cache(&mut self, width: u16) {
        if let Some(stream) = self.stream_mut() {
            stream.ensure_render_cache(width);
        }
    }

    pub(crate) fn desired_height(&self) -> u16 {
        match self {
            Self::WorkingSpinner { .. } => 3,
            Self::AssistantStreaming { stream } => stream.desired_height(),
            Self::SlashMenu | Self::FilePicker => 3,
        }
    }

    pub(crate) fn spill_overflow_rows(&mut self, visible_rows: usize) -> Option<String> {
        let stream = self.stream_mut()?;
        stream.spill_overflow_rows(visible_rows)
    }

    pub(crate) fn tick(&mut self) {
        if let Self::WorkingSpinner { spinner_frame, .. } = self {
            *spinner_frame = spinner_frame.wrapping_add(1);
        }
    }

    pub(crate) fn spinner_frame(&self) -> &'static str {
        let frame = match self {
            Self::WorkingSpinner { spinner_frame, .. } => *spinner_frame,
            _ => 0,
        };
        SPINNER_FRAMES[frame % SPINNER_FRAMES.len()]
    }

    pub(crate) fn push_assistant_delta(&mut self, chunk: &str) {
        if chunk.is_empty() || !self.is_spillable() {
            return;
        }

        let current = std::mem::replace(
            self,
            Self::AssistantStreaming {
                stream: ActiveStreamState::new(),
            },
        );

        let mut stream = match current {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => stream,
            other => {
                *self = other;
                return;
            }
        };

        stream.push_delta(chunk);
        *self = Self::AssistantStreaming { stream };
    }

    pub(crate) fn into_unfrozen_transcript_text(self) -> Option<String> {
        let stream = match self {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => stream,
            Self::SlashMenu | Self::FilePicker => return None,
        };

        let text = stream.into_unfrozen_text();
        if text.is_empty() { None } else { Some(text) }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveStreamState {
    full_text: String,
    frozen_until: usize,
    revision: u64,
    render_cache: Option<ActiveStreamRenderCache>,
}

impl ActiveStreamState {
    pub(crate) fn new() -> Self {
        Self {
            full_text: String::new(),
            frozen_until: 0,
            revision: 0,
            render_cache: None,
        }
    }

    pub(crate) fn full_text(&self) -> &str {
        &self.full_text
    }

    pub(crate) fn frozen_until(&self) -> usize {
        self.frozen_until
    }

    pub(crate) fn active_tail(&self) -> &str {
        &self.full_text[self.frozen_until..]
    }

    pub(crate) fn rendered_tail_rows(&self) -> Option<&[Line<'static>]> {
        self.render_cache
            .as_ref()
            .map(|cache| cache.body_rows.as_slice())
    }

    pub(crate) fn push_delta(&mut self, chunk: &str) {
        self.full_text.push_str(chunk);
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn advance_frozen_until(&mut self, mut next: usize) {
        next = next.min(self.full_text.len());
        while next > 0 && !self.full_text.is_char_boundary(next) {
            next -= 1;
        }

        if next > self.frozen_until {
            self.frozen_until = next;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn desired_height(&self) -> u16 {
        let body_rows = self
            .render_cache
            .as_ref()
            .map_or(1, |cache| cache.body_rows.len());
        let top_padding = if self.has_spilled_content() { 0 } else { 1 };
        u16::try_from(body_rows.saturating_add(top_padding))
            .unwrap_or(u16::MAX)
            .max(1)
    }

    fn spill_overflow_rows(&mut self, visible_rows: usize) -> Option<String> {
        let top_padding = if self.has_spilled_content() { 0 } else { 1 };
        let body_capacity = visible_rows.saturating_sub(top_padding).max(1);
        let cache = self.render_cache.as_ref()?;
        if cache.body_rows.len() <= body_capacity {
            return None;
        }

        let row_count = cache.body_rows.len();
        let stable_rows = row_count.saturating_sub(ACTIVE_STREAM_UNSTABLE_TAIL_ROWS);
        let desired_spill = row_count - body_capacity;
        let spill_rows = desired_spill.min(stable_rows);
        if spill_rows == 0 {
            return None;
        }

        let boundary = cache.row_end_boundaries[spill_rows - 1];
        let tail_bytes = self.boundary_to_tail_byte(boundary);
        if tail_bytes == 0 {
            return None;
        }

        let mut next_frozen = self.frozen_until + tail_bytes;
        if next_frozen < self.full_text.len() && self.full_text[next_frozen..].starts_with('\n') {
            next_frozen += '\n'.len_utf8();
        }

        let fragment = self.full_text[self.frozen_until..next_frozen].to_string();
        self.advance_frozen_until(next_frozen);
        Some(fragment)
    }

    fn ensure_render_cache(&mut self, width: u16) {
        let width = width.max(1);
        let key = (self.revision, width);
        if self
            .render_cache
            .as_ref()
            .is_some_and(|cache| cache.key == key)
        {
            return;
        }

        let logical_rows = TuiFormatter.format_assistant_body(self.active_tail());
        let wrapped =
            wrap_transcript_rows(width, &logical_rows, TranscriptCommitBoundary::default());
        self.render_cache = Some(ActiveStreamRenderCache {
            key,
            body_rows: wrapped.rows,
            row_end_boundaries: wrapped.row_end_boundaries,
        });
    }

    fn has_spilled_content(&self) -> bool {
        self.frozen_until > 0
    }

    fn boundary_to_tail_byte(&self, boundary: TranscriptCommitBoundary) -> usize {
        let tail = self.active_tail();
        let mut byte = 0usize;
        for (row, line) in tail.split('\n').enumerate() {
            if row == boundary.logical_row {
                return (byte + boundary.byte_offset.min(line.len())).min(tail.len());
            }
            byte = byte.saturating_add(line.len()).saturating_add(1);
        }
        tail.len()
    }

    fn into_unfrozen_text(mut self) -> String {
        if self.frozen_until >= self.full_text.len() {
            return String::new();
        }

        self.full_text.split_off(self.frozen_until)
    }
}

#[derive(Debug, Clone)]
struct ActiveStreamRenderCache {
    key: (u64, u16),
    body_rows: Vec<Line<'static>>,
    row_end_boundaries: Vec<TranscriptCommitBoundary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivePaneKind {
    WorkingSpinner,
    AssistantStreaming,
    SlashMenu,
    FilePicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveBehavior {
    SpillToTranscript,
    OccludeOnly,
}

#[derive(Debug)]
pub(crate) struct ActiveTicker {
    interval: Duration,
    next_tick: Instant,
    animating: bool,
}

impl ActiveTicker {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_tick: Instant::now() + interval,
            animating: false,
        }
    }

    pub(crate) fn wait_timeout(
        &mut self,
        now: Instant,
        active: Option<&ActivePaneState>,
        idle_timeout: Duration,
    ) -> Duration {
        if !is_spinner_animating(active) {
            self.animating = false;
            self.next_tick = now + self.interval;
            return idle_timeout;
        }

        if !self.animating {
            self.animating = true;
            self.next_tick = now + self.interval;
        }

        self.next_tick.saturating_duration_since(now)
    }

    pub(crate) fn tick_if_due(
        &mut self,
        now: Instant,
        active: Option<&mut ActivePaneState>,
    ) -> bool {
        let Some(active) = active else {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        };

        if active.kind() != ActivePaneKind::WorkingSpinner {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        }

        if now < self.next_tick {
            return false;
        }

        active.tick();
        self.next_tick = now + self.interval;
        true
    }
}

fn is_spinner_animating(active: Option<&ActivePaneState>) -> bool {
    matches!(
        active.map(|active| active.kind()),
        Some(ActivePaneKind::WorkingSpinner)
    )
}
