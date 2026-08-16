//! Provider-independent native execution primitives.
//!
//! The compatibility agent still lives under [`crate::agent`].  This module is
//! the stable native state boundary used by later hosts and bindings.

mod model_turn;
mod session;

pub use session::{
    AgentMessage, CustomEntryError, KernelSession, SessionEntry, SessionMessageError,
    SessionSnapshot,
};

pub use crate::agent::tool_call::{AssembledToolCall, InvalidToolCall, ToolCallRequest};
pub use model_turn::{ModelTurn, ModelTurnError, ModelTurnResult, ModelTurnState};

pub use iyon_api::{ContentBlock, ModelMessage, StopReason, Usage};
