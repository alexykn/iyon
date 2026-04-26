#[derive(Debug, Clone)]
pub enum CoreCommand {
    SubmitTurn {
        text: String,
    },
    CancelActiveTurn,
    ApproveToolCall {
        approval_id: u64,
    },
    RejectToolCall {
        approval_id: u64,
        reason: Option<String>,
    },
    Shutdown,
}
