use serde_json::Value;
use thiserror::Error;

use super::{ApprovalDecision, ApprovalRequirement, ApprovalState, ApprovalStatus};
use crate::{
    agent::tool_call::AssembledToolCall,
    ids::{ApprovalId, ToolCallId},
    kernel::ContentBlock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolLifecycleState {
    Preparing,
    Prepared,
    Running,
    PendingApproval,
    Finished,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct ToolLifecycleResult {
    pub content: Vec<ContentBlock>,
    pub details: Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLifecycleEvent {
    pub sequence: u64,
    pub tool_call_id: ToolCallId,
    pub state: ToolLifecycleState,
    pub approval_id: Option<ApprovalId>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolLifecycleError {
    #[error("invalid tool lifecycle transition from {from:?} using {operation}")]
    InvalidTransition {
        from: ToolLifecycleState,
        operation: &'static str,
    },
    #[error("stale approval id {0:?}")]
    StaleApproval(ApprovalId),
    #[error("approval id {0:?} has already been resolved")]
    DuplicateApprovalResolution(ApprovalId),
}

#[derive(Debug, Clone)]
pub struct ToolLifecycleHandle {
    call: AssembledToolCall,
    state: ToolLifecycleState,
    prepared_arguments: Option<Value>,
    result: Option<ToolLifecycleResult>,
    error: Option<String>,
    cancellation_reason: Option<String>,
    approval: Option<ApprovalState>,
    approval_started_running: bool,
    next_approval_id: u64,
    sequence: u64,
    events: Vec<ToolLifecycleEvent>,
}

impl ToolLifecycleHandle {
    pub fn new(call: AssembledToolCall) -> Self {
        let mut handle = Self {
            call,
            state: ToolLifecycleState::Preparing,
            prepared_arguments: None,
            result: None,
            error: None,
            cancellation_reason: None,
            approval: None,
            approval_started_running: false,
            next_approval_id: 1,
            sequence: 0,
            events: Vec::new(),
        };
        handle.record_event();
        handle
    }

    pub fn call(&self) -> &AssembledToolCall {
        &self.call
    }
    pub fn state(&self) -> ToolLifecycleState {
        self.state
    }
    pub fn prepared_arguments(&self) -> Option<&Value> {
        self.prepared_arguments.as_ref()
    }
    pub fn result(&self) -> Option<&ToolLifecycleResult> {
        self.result.as_ref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn cancellation_reason(&self) -> Option<&str> {
        self.cancellation_reason.as_deref()
    }
    pub fn approval(&self) -> Option<&ApprovalState> {
        self.approval.as_ref()
    }
    pub fn events(&self) -> &[ToolLifecycleEvent] {
        &self.events
    }

    pub fn prepare(&mut self) -> Result<(), ToolLifecycleError> {
        self.require_state(ToolLifecycleState::Preparing, "prepare")
    }

    pub fn prepared(&mut self, arguments: Value) -> Result<(), ToolLifecycleError> {
        self.require_state(ToolLifecycleState::Preparing, "prepared")?;
        self.prepared_arguments = Some(arguments);
        self.state = ToolLifecycleState::Prepared;
        self.record_event();
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), ToolLifecycleError> {
        self.require_state(ToolLifecycleState::Prepared, "start")?;
        self.state = ToolLifecycleState::Running;
        self.record_event();
        Ok(())
    }

    pub fn request_approval(
        &mut self,
        requirement: ApprovalRequirement,
    ) -> Result<Option<ApprovalId>, ToolLifecycleError> {
        if !matches!(
            self.state,
            ToolLifecycleState::Prepared | ToolLifecycleState::Running
        ) {
            return Err(ToolLifecycleError::InvalidTransition {
                from: self.state,
                operation: "request_approval",
            });
        }
        if requirement == ApprovalRequirement::NotRequired {
            return Ok(None);
        }
        let id = ApprovalId(self.next_approval_id);
        self.next_approval_id = self.next_approval_id.saturating_add(1);
        self.approval = Some(ApprovalState {
            id,
            requirement,
            status: ApprovalStatus::Pending,
        });
        self.approval_started_running = self.state == ToolLifecycleState::Running;
        self.state = ToolLifecycleState::PendingApproval;
        self.record_event();
        Ok(Some(id))
    }

    pub fn resolve_approval(
        &mut self,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    ) -> Result<(), ToolLifecycleError> {
        let Some(approval) = self.approval.as_mut() else {
            return Err(ToolLifecycleError::StaleApproval(approval_id));
        };
        if approval.id != approval_id {
            return Err(ToolLifecycleError::StaleApproval(approval_id));
        }
        if approval.status != ApprovalStatus::Pending {
            return Err(ToolLifecycleError::DuplicateApprovalResolution(approval_id));
        }
        if self.state != ToolLifecycleState::PendingApproval {
            return Err(ToolLifecycleError::InvalidTransition {
                from: self.state,
                operation: "resolve_approval",
            });
        }
        match decision {
            ApprovalDecision::Approved => {
                approval.status = ApprovalStatus::Approved;
                self.state = if self.approval_started_running {
                    ToolLifecycleState::Running
                } else {
                    ToolLifecycleState::Prepared
                };
            }
            ApprovalDecision::Rejected { reason } => {
                approval.status = ApprovalStatus::Rejected {
                    reason: reason.clone(),
                };
                self.error = Some(reason.unwrap_or_else(|| "tool approval rejected".to_string()));
                self.state = ToolLifecycleState::Failed;
            }
        }
        self.record_event();
        Ok(())
    }

    pub fn finish(&mut self, result: ToolLifecycleResult) -> Result<(), ToolLifecycleError> {
        self.require_state(ToolLifecycleState::Running, "finish")?;
        self.result = Some(result);
        self.state = ToolLifecycleState::Finished;
        self.record_event();
        Ok(())
    }

    pub fn fail(&mut self, error: impl Into<String>) -> Result<(), ToolLifecycleError> {
        if matches!(
            self.state,
            ToolLifecycleState::Finished
                | ToolLifecycleState::Failed
                | ToolLifecycleState::Cancelled
        ) {
            return Err(ToolLifecycleError::InvalidTransition {
                from: self.state,
                operation: "fail",
            });
        }
        self.error = Some(error.into());
        self.state = ToolLifecycleState::Failed;
        if let Some(approval) = self.approval.as_mut() {
            if approval.is_pending() {
                approval.status = ApprovalStatus::Cancelled;
            }
        }
        self.record_event();
        Ok(())
    }

    pub fn cancel(&mut self, reason: impl Into<String>) -> Result<(), ToolLifecycleError> {
        if self.state == ToolLifecycleState::Cancelled {
            return Ok(());
        }
        if matches!(
            self.state,
            ToolLifecycleState::Finished | ToolLifecycleState::Failed
        ) {
            return Err(ToolLifecycleError::InvalidTransition {
                from: self.state,
                operation: "cancel",
            });
        }
        self.cancellation_reason = Some(reason.into());
        self.state = ToolLifecycleState::Cancelled;
        if let Some(approval) = self.approval.as_mut() {
            if approval.is_pending() {
                approval.status = ApprovalStatus::Cancelled;
            }
        }
        self.record_event();
        Ok(())
    }

    fn require_state(
        &self,
        expected: ToolLifecycleState,
        operation: &'static str,
    ) -> Result<(), ToolLifecycleError> {
        if self.state == expected {
            return Ok(());
        }
        Err(ToolLifecycleError::InvalidTransition {
            from: self.state,
            operation,
        })
    }

    fn record_event(&mut self) {
        let sequence = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        self.events.push(ToolLifecycleEvent {
            sequence,
            tool_call_id: self.call.id.clone(),
            state: self.state,
            approval_id: self.approval.as_ref().map(|approval| approval.id),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolLifecycleError, ToolLifecycleHandle, ToolLifecycleResult, ToolLifecycleState};
    use crate::{
        agent::tool_call::AssembledToolCall,
        kernel::{ApprovalDecision, ApprovalRequirement},
    };
    use serde_json::json;

    fn lifecycle() -> ToolLifecycleHandle {
        ToolLifecycleHandle::new(AssembledToolCall {
            id: crate::ids::ToolCallId("call-1".into()),
            name: "read".into(),
            arguments: json!({}),
        })
    }
    fn result() -> ToolLifecycleResult {
        ToolLifecycleResult {
            content: vec![],
            details: json!({"ok": true}),
            is_error: false,
        }
    }

    #[test]
    fn tool_lifecycle_accepts_prepared_running_and_finished() {
        let mut l = lifecycle();
        l.prepare().unwrap();
        l.prepared(json!({})).unwrap();
        l.start().unwrap();
        l.finish(result()).unwrap();
        assert_eq!(l.state(), ToolLifecycleState::Finished);
    }
    #[test]
    fn approval_can_return_to_running_only_after_matching_resolution() {
        let mut l = lifecycle();
        l.prepared(json!({})).unwrap();
        l.start().unwrap();
        let id = l
            .request_approval(ApprovalRequirement::Required { reason: None })
            .unwrap()
            .unwrap();
        assert_eq!(l.state(), ToolLifecycleState::PendingApproval);
        l.resolve_approval(id, ApprovalDecision::Approved).unwrap();
        assert_eq!(l.state(), ToolLifecycleState::Running);
    }
    #[test]
    fn approval_rejection_finishes_as_rejected_failure() {
        let mut l = lifecycle();
        l.prepared(json!({})).unwrap();
        l.start().unwrap();
        let id = l
            .request_approval(ApprovalRequirement::Required { reason: None })
            .unwrap()
            .unwrap();
        l.resolve_approval(
            id,
            ApprovalDecision::Rejected {
                reason: Some("no".into()),
            },
        )
        .unwrap();
        assert_eq!(l.state(), ToolLifecycleState::Failed);
        assert!(l.finish(result()).is_err());
    }
    #[test]
    fn cancel_is_terminal_and_idempotent() {
        let mut l = lifecycle();
        l.cancel("interrupt").unwrap();
        l.cancel("interrupt again").unwrap();
        assert_eq!(l.state(), ToolLifecycleState::Cancelled);
    }
    #[test]
    fn invalid_tool_transition_returns_typed_error() {
        let mut l = lifecycle();
        assert!(matches!(
            l.finish(result()).unwrap_err(),
            ToolLifecycleError::InvalidTransition { .. }
        ));
    }
    #[test]
    fn stale_approval_resolution_is_rejected() {
        let mut l = lifecycle();
        l.prepared(json!({})).unwrap();
        l.start().unwrap();
        l.request_approval(ApprovalRequirement::Required { reason: None })
            .unwrap();
        assert!(matches!(
            l.resolve_approval(crate::ids::ApprovalId(99), ApprovalDecision::Approved)
                .unwrap_err(),
            ToolLifecycleError::StaleApproval(_)
        ));
    }
    #[test]
    fn lifecycle_events_match_state_transitions() {
        let mut l = lifecycle();
        l.prepared(json!({})).unwrap();
        l.start().unwrap();
        l.finish(result()).unwrap();
        let states: Vec<_> = l.events().iter().map(|event| event.state).collect();
        assert_eq!(
            states,
            vec![
                ToolLifecycleState::Preparing,
                ToolLifecycleState::Prepared,
                ToolLifecycleState::Running,
                ToolLifecycleState::Finished
            ]
        );
    }
}
