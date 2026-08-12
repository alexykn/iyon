use super::backend::FrontendEvent;
use super::components::ApprovalDecision;

#[derive(Debug)]
pub enum IyonAction {
    SubmitTurn(String),
    ToolApproval(ApprovalDecision),
    CtrlC,
    Escape,
    RequestExit,
    InterruptActiveTurn,
    CycleReasoningEffort,
    ComposerPaste(String),
    Backend(FrontendEvent),
}
