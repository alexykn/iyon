use crate::ids::ApprovalId;

#[derive(Debug)]
pub(crate) enum AgentLoopControl {
    Cancel,
    Steer { text: String },
    ApproveToolCall {
        approval_id: ApprovalId,
    },
    RejectToolCall {
        approval_id: ApprovalId,
        reason: Option<String>,
    },
}
