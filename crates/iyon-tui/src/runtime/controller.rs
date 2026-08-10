//! Application action vocabulary.
//!
//! These are semantic application actions. Component-local editing and
//! approval commands never cross this boundary as framework commands.

use super::final_components::ApprovalDecision;

#[derive(Debug)]
pub(crate) enum AppAction {
    SubmitTurn(String),
    RequestExit,
    InterruptActiveTurn,
    CycleReasoningEffort,
    ToolApproval(ApprovalDecision),
}
