use anyhow::Result;

use crate::runtime::{
    AppState,
    backend::{BackendEventHandler, SubmitTurnResult},
};

#[derive(Debug)]
pub(crate) enum FrontendAction {
    SubmitTurn { text: String },
    RequestExit,
    RedrawOnly,
    ApprovePendingTool,
    RejectPendingTool,
    CycleReasoningEffort,
    InterruptActiveTurn,
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
                match backend.try_submit_turn(text)? {
                    SubmitTurnResult::Sent => {}
                    SubmitTurnResult::NoBackendAttached => {
                        state.fail_active_turn("backend unavailable".to_string());
                    }
                }
                Ok(true)
            }
            FrontendAction::RequestExit => {
                if state.active().is_some() || state.streaming_ref().is_some() {
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
            FrontendAction::CycleReasoningEffort => {
                // Optimistic: draw the next effort a frame ahead so the UI never
                // looks like it ignored the key while core round-trips.
                state.cycle_reasoning_effort();
                backend.try_cycle_reasoning_effort()?;
                Ok(true)
            }
            FrontendAction::InterruptActiveTurn => {
                if state.active().is_some()
                    || state.streaming_ref().is_some()
                    || state.pending_tool_approval.is_some()
                {
                    backend.try_cancel_active_turn()?;
                }
                Ok(true)
            }
        }
    }
}
