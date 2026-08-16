use tokio_util::sync::CancellationToken;

use super::{ApprovalRequirement, KernelQueues, KernelSession};
use crate::{ids::SessionId, tools::ToolRegistry};

/// Inputs owned by the host when constructing a native kernel.
#[derive(Clone)]
pub struct KernelConfig {
    pub session: KernelSession,
    pub tools: ToolRegistry,
    pub approval_requirement: ApprovalRequirement,
    pub command_capacity: usize,
    pub event_capacity: usize,
    pub cancellation: CancellationToken,
    /// An optional host safety ceiling. Product policies such as the legacy
    /// 16-tool rule do not belong here.
    pub host_tool_call_ceiling: Option<usize>,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            session: KernelSession::new(SessionId(1)),
            tools: ToolRegistry::new(),
            approval_requirement: ApprovalRequirement::NotRequired,
            command_capacity: 32,
            event_capacity: 128,
            cancellation: CancellationToken::new(),
            host_tool_call_ceiling: None,
        }
    }
}

impl KernelConfig {
    pub fn new(session: KernelSession, tools: ToolRegistry) -> Self {
        Self {
            session,
            tools,
            ..Self::default()
        }
    }

    pub fn with_approval_requirement(mut self, requirement: ApprovalRequirement) -> Self {
        self.approval_requirement = requirement;
        self
    }

    pub fn with_capacities(mut self, command_capacity: usize, event_capacity: usize) -> Self {
        self.command_capacity = command_capacity.max(1);
        self.event_capacity = event_capacity.max(1);
        self
    }

    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    pub fn with_host_tool_call_ceiling(mut self, ceiling: Option<usize>) -> Self {
        self.host_tool_call_ceiling = ceiling;
        self
    }
}

#[derive(Clone)]
pub struct Kernel {
    session: KernelSession,
    tools: ToolRegistry,
    approval_requirement: ApprovalRequirement,
    command_capacity: usize,
    event_capacity: usize,
    cancellation: CancellationToken,
    host_tool_call_ceiling: Option<usize>,
    queues: KernelQueues,
}

impl Kernel {
    pub fn new(config: KernelConfig) -> Self {
        let cancellation = config.cancellation.clone();
        Self {
            session: config.session,
            tools: config.tools,
            approval_requirement: config.approval_requirement,
            command_capacity: config.command_capacity.max(1),
            event_capacity: config.event_capacity.max(1),
            cancellation: config.cancellation,
            host_tool_call_ceiling: config.host_tool_call_ceiling,
            queues: KernelQueues::new(config.command_capacity, cancellation),
        }
    }

    pub fn session(&self) -> &KernelSession {
        &self.session
    }
    pub fn session_mut(&mut self) -> &mut KernelSession {
        &mut self.session
    }
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }
    pub fn approval_requirement(&self) -> &ApprovalRequirement {
        &self.approval_requirement
    }
    pub fn command_capacity(&self) -> usize {
        self.command_capacity
    }
    pub fn event_capacity(&self) -> usize {
        self.event_capacity
    }
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
    pub fn host_tool_call_ceiling(&self) -> Option<usize> {
        self.host_tool_call_ceiling
    }
    pub fn queues(&self) -> &KernelQueues {
        &self.queues
    }
    pub fn queues_mut(&mut self) -> &mut KernelQueues {
        &mut self.queues
    }
}

#[cfg(test)]
mod tests {
    use super::{Kernel, KernelConfig};
    use crate::{ids::SessionId, kernel::KernelSession, tools::ToolRegistry};

    #[test]
    fn kernel_construction_uses_only_injected_registry() {
        let kernel = Kernel::new(KernelConfig::new(
            KernelSession::new(SessionId(9)),
            ToolRegistry::new(),
        ));
        assert!(kernel.tools().definitions().is_empty());
        assert_eq!(kernel.session().id(), SessionId(9));
    }

    #[test]
    fn empty_injected_registry_has_no_implicit_builtins() {
        let kernel = Kernel::new(KernelConfig::default());
        assert!(kernel.tools().model_specs().is_empty());
    }
}
