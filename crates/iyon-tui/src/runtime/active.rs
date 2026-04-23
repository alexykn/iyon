use std::time::{Duration, Instant};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

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
}

impl ActiveStreamState {
    pub(crate) fn new() -> Self {
        Self {
            full_text: String::new(),
            frozen_until: 0,
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

    pub(crate) fn push_delta(&mut self, chunk: &str) {
        self.full_text.push_str(chunk);
    }

    pub(crate) fn advance_frozen_until(&mut self, mut next: usize) {
        next = next.min(self.full_text.len());
        while next > 0 && !self.full_text.is_char_boundary(next) {
            next -= 1;
        }

        if next > self.frozen_until {
            self.frozen_until = next;
        }
    }

    fn into_unfrozen_text(mut self) -> String {
        if self.frozen_until >= self.full_text.len() {
            return String::new();
        }

        self.full_text.split_off(self.frozen_until)
    }
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
