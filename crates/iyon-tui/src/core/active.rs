const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone)]
pub(crate) struct ActivePaneState {
    kind: ActivePaneKind,
    behavior: ActiveBehavior,
    stream: Option<ActiveStreamState>,
    spinner_frame: usize,
}

impl ActivePaneState {
    pub(crate) fn working_spinner() -> Self {
        Self {
            kind: ActivePaneKind::WorkingSpinner,
            behavior: ActiveBehavior::SpillToTranscript,
            stream: Some(ActiveStreamState::new()),
            spinner_frame: 0,
        }
    }

    pub(crate) fn kind(&self) -> ActivePaneKind {
        self.kind
    }

    pub(crate) fn behavior(&self) -> ActiveBehavior {
        self.behavior
    }

    pub(crate) fn is_spillable(&self) -> bool {
        self.behavior == ActiveBehavior::SpillToTranscript
    }

    pub(crate) fn stream(&self) -> Option<&ActiveStreamState> {
        self.stream.as_ref()
    }

    pub(crate) fn stream_mut(&mut self) -> Option<&mut ActiveStreamState> {
        self.stream.as_mut()
    }

    pub(crate) fn tick(&mut self) {
        if self.kind == ActivePaneKind::WorkingSpinner {
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
        }
    }

    pub(crate) fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()]
    }

    pub(crate) fn push_assistant_delta(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }

        debug_assert_eq!(self.behavior, ActiveBehavior::SpillToTranscript);
        if !self.is_spillable() {
            return;
        }

        self.kind = ActivePaneKind::AgentStreaming;
        self.stream
            .get_or_insert_with(ActiveStreamState::new)
            .push_delta(chunk);
    }

    pub(crate) fn into_unfrozen_transcript_text(self) -> Option<String> {
        if self.behavior != ActiveBehavior::SpillToTranscript {
            return None;
        }

        let stream = self.stream?;
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
    AgentStreaming,
    SlashMenu,
    FilePicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveBehavior {
    SpillToTranscript,
    OccludeOnly,
}
