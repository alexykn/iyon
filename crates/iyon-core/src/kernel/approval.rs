use crate::ids::ApprovalId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalRequirement {
    NotRequired,
    Required { reason: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected { reason: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected { reason: Option<String> },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalState {
    pub id: ApprovalId,
    pub requirement: ApprovalRequirement,
    pub status: ApprovalStatus,
}

impl ApprovalState {
    pub fn is_pending(&self) -> bool {
        self.status == ApprovalStatus::Pending
    }
}
