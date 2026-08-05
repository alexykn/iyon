#[derive(Debug, Clone)]
pub enum CoreCommand {
    SubmitTurn {
        text: String,
    },
    CancelActiveTurn,
    // Cycle the session's reasoning effort to the next provider-supported level.
    CycleReasoningEffort,
    ApproveToolCall {
        approval_id: u64,
    },
    RejectToolCall {
        approval_id: u64,
        reason: Option<String>,
    },
    Shutdown,
}
