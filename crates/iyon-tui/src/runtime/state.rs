use crate::{
    input::InputBuffer,
    runtime::active::ActivePaneState,
    transcript::{TimelineItem, TranscriptState},
};

#[derive(Debug, Default)]
pub(crate) struct AppState {
    pub(crate) input: InputBuffer,
    pub(crate) transcript: TranscriptState,
    pub(crate) active: Option<ActivePaneState>,

    pub(crate) input_view: InputViewState,
    pub(crate) info: InfoState,

    pub(crate) exit_state: ExitState,
}

impl AppState {
    pub(crate) fn append_timeline_item(&mut self, item: TimelineItem) {
        self.transcript.push_item(item);
    }

    pub(crate) fn append_assistant_message(&mut self, text: String) {
        self.transcript.append_assistant_fragment(text);
    }

    pub(crate) fn append_error_message(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.transcript
            .push_item(TimelineItem::ErrorMessage { text });
    }

    pub(crate) fn submit_user_message(&mut self, text: String) {
        self.append_timeline_item(TimelineItem::UserMessage { text });
        self.start_working_pane();
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

    pub(crate) fn take_active_unfrozen_transcript_text(&mut self) -> Option<String> {
        self.active
            .take()
            .and_then(ActivePaneState::into_unfrozen_transcript_text)
    }

    pub(crate) fn finish_active_turn(&mut self) {
        if let Some(text) = self.take_active_unfrozen_transcript_text() {
            self.append_assistant_message(text);
        }
    }

    pub(crate) fn fail_active_turn(&mut self, message: String) {
        self.finish_active_turn();
        self.append_error_message(message);
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct InputViewState {
    pub(crate) content_width: u16,
    pub(crate) scroll_rows: u16,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InfoState {
    pub(crate) status: String,
}
