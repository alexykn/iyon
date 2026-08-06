use crate::ids::ApprovalId;

#[derive(Debug)]
pub(crate) enum AgentLoopControl {
    ApproveToolCall {
        approval_id: ApprovalId,
    },
    RejectToolCall {
        approval_id: ApprovalId,
        reason: Option<String>,
    },
}
