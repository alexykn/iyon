//! Provider-independent native execution primitives.
//!
//! The compatibility agent still lives under [`crate::agent`].  This module is
//! the stable native state boundary used by later hosts and bindings.

mod session;

pub use session::{
    AgentMessage, CustomEntryError, KernelSession, SessionEntry, SessionMessageError,
    SessionSnapshot,
};

pub use iyon_api::{ContentBlock, ModelMessage, StopReason, Usage};
