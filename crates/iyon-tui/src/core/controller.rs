use anyhow::Result;

use crate::core::{backend::BackendEventHandler, state::AppState};

#[derive(Debug)]
pub(crate) enum FrontendAction {
    SubmitTurn { text: String },
    RequestExit,
    RedrawOnly,
    BackendTurnStarted,
    AssistantDelta { text: String },
    BackendTurnFinished,
    BackendTurnFailed { message: String },
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

    fn apply(
        &mut self,
        action: FrontendAction,
        state: &mut AppState,
        backend: &mut BackendEventHandler,
    ) -> Result<bool> {
        match action {
            FrontendAction::SubmitTurn { text } => {
                state.submit_user_message(text.clone());
                if let Err(error) = backend.try_submit_turn(text) {
                    state.fail_active_turn(format!("backend unavailable: {error:#}"));
                }
                Ok(true)
            }
            FrontendAction::RequestExit => {
                state.request_exit();
                Ok(true)
            }
            FrontendAction::RedrawOnly => Ok(true),
            FrontendAction::BackendTurnStarted => {
                state.start_working_pane();
                Ok(true)
            }
            FrontendAction::AssistantDelta { text } => {
                state.receive_assistant_delta(&text);
                Ok(true)
            }
            FrontendAction::BackendTurnFinished => {
                state.finish_active_turn();
                Ok(true)
            }
            FrontendAction::BackendTurnFailed { message } => {
                state.fail_active_turn(message);
                Ok(true)
            }
        }
    }
}
