use anyhow::Result;

use crate::runtime::{
    AppState,
    active::ActiveBehavior,
    backend::{BackendEventHandler, SubmitTurnResult},
};

#[derive(Debug)]
pub(crate) enum FrontendAction {
    SubmitTurn { text: String },
    RequestExit,
    RedrawOnly,
    ApprovePendingTool,
    RejectPendingTool,
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

    pub(crate) fn spill_active_into_transcript_if_needed(
        &mut self,
        state: &mut AppState,
        visible_rows: usize,
    ) -> bool {
        if !matches!(
            state
                .active
                .as_ref()
                .map(super::active::ActivePaneState::behavior),
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
                match backend.try_submit_turn(text)? {
                    SubmitTurnResult::Sent => {}
                    SubmitTurnResult::NoBackendAttached => {
                        state.fail_active_turn("backend unavailable".to_string());
                    }
                }
                Ok(true)
            }
            FrontendAction::RequestExit => {
                if state.active.is_some() {
                    backend.try_cancel_active_turn()?;
                } else {
                    state.request_exit();
                }
                Ok(true)
            }
            FrontendAction::RedrawOnly => Ok(true),
            FrontendAction::ApprovePendingTool => {
                if let Some(pending) = state.pending_tool_approval.as_ref() {
                    backend.try_approve_tool_call(pending.approval_id)?;
                }
                Ok(true)
            }
            FrontendAction::RejectPendingTool => {
                if let Some(pending) = state.pending_tool_approval.as_ref() {
                    backend.try_reject_tool_call(
                        pending.approval_id,
                        Some("Rejected by user".to_string()),
                    )?;
                }
                Ok(true)
            }
        }
    }
}
