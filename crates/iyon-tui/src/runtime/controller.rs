use anyhow::Result;

use crate::runtime::{
    AppState,
    active::ActiveBehavior,
    backend::{BackendEventHandler, FrontendEvent, SubmitTurnResult},
};

#[derive(Debug)]
pub(crate) enum FrontendAction {
    SubmitTurn { text: String },
    RequestExit,
    RedrawOnly,
}

#[derive(Debug, Default)]
pub(crate) struct AppController;

impl AppController {
    pub(crate) fn apply_all(
        &mut self,
        actions: impl IntoIterator<Item = FrontendAction>,
        state: &mut AppState,
        backend: &mut BackendEventHandler,
    ) -> Result<bool> {
        let mut dirty = false;
        for action in actions {
            dirty |= self.apply(action, state, backend)?;
        }
        Ok(dirty)
    }

    pub(crate) fn apply_backend_events(
        &mut self,
        events: impl IntoIterator<Item = FrontendEvent>,
        state: &mut AppState,
    ) -> Result<bool> {
        let mut dirty = false;
        for event in events {
            dirty |= self.apply_backend_event(event, state);
        }
        Ok(dirty)
    }

    pub(crate) fn spill_active_into_transcript_if_needed(
        &mut self,
        state: &mut AppState,
        visible_rows: usize,
    ) -> bool {
        if !matches!(
            state.active.as_ref().map(|pane| pane.behavior()),
            Some(ActiveBehavior::SpillToTranscript)
        ) {
            return false;
        }

        let Some(fragment) = state
            .active
            .as_mut()
            .and_then(|pane| pane.spill_overflow_rows(visible_rows))
        else {
            return false;
        };

        state.append_assistant_stream_fragment(fragment);
        true
    }

    fn apply(
        &mut self,
        action: FrontendAction,
        state: &mut AppState,
        backend: &mut BackendEventHandler,
    ) -> Result<bool> {
        match action {
            FrontendAction::SubmitTurn { text } => {
                state.submit_user_message(text.clone());
                match backend.try_submit_turn(text)? {
                    SubmitTurnResult::Sent => {}
                    SubmitTurnResult::NoBackendAttached => {
                        state.fail_active_turn("backend unavailable".to_string());
                    }
                }
                Ok(true)
            }
            FrontendAction::RequestExit => {
                state.request_exit();
                Ok(true)
            }
            FrontendAction::RedrawOnly => Ok(true),
        }
    }

    fn apply_backend_event(&mut self, event: FrontendEvent, state: &mut AppState) -> bool {
        match event {
            FrontendEvent::TurnStarted => {
                state.start_working_pane();
                true
            }
            FrontendEvent::AssistantDelta { text } => {
                state.receive_assistant_delta(&text);
                true
            }
            FrontendEvent::TurnFinished => {
                state.finish_active_turn();
                true
            }
            FrontendEvent::TurnFailed { message } => {
                state.fail_active_turn(message);
                true
            }
        }
    }
}
