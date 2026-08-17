//! Provider-independent native execution primitives.
//!
//! The compatibility agent still lives under [`crate::agent`].  This module is
//! the stable native state boundary used by later hosts and bindings.

mod approval;
mod kernel;
mod model_turn;
mod queue;
mod session;
mod tool;

#[cfg(test)]
mod tests;

pub use session::{
    AgentMessage, CustomEntryError, KernelSession, SessionEntry, SessionMessageError,
    SessionSnapshot,
};

pub use crate::agent::tool_call::{AssembledToolCall, InvalidToolCall, ToolCallRequest};
pub use approval::{ApprovalDecision, ApprovalRequirement, ApprovalState, ApprovalStatus};
pub use kernel::{Kernel, KernelConfig};
pub use model_turn::{ModelTurn, ModelTurnError, ModelTurnResult, ModelTurnState};
pub use queue::{KernelQueues, QueueFull, QueueIdAllocator, QueueKind, QueueOperation};
pub use tool::{
    ToolLifecycleError, ToolLifecycleEvent, ToolLifecycleHandle, ToolLifecycleResult,
    ToolLifecycleState,
};

pub use iyon_api::{ContentBlock, ModelMessage, StopReason, Usage};
