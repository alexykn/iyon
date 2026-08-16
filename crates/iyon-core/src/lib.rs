pub(crate) mod agent;
mod command;
mod event;
pub(crate) mod fs;
mod handle;
pub mod ids;
pub mod kernel;
mod runtime;
pub(crate) mod session;
pub mod tools;

pub use command::CoreCommand;
pub use event::{CoreEvent, MessageDelta, MessageRole, ToolCallDelta, ToolUpdateEvent};
pub use handle::{CoreCommandSender, CoreEventReceiver, IyonCore};
pub use ids::{ApprovalId, MessageId, SessionId, ToolCallId, TurnId};
pub use iyon_api::ReasoningLevel;
pub use session::state::{ModelSelection, SessionState};

pub fn spawn() -> anyhow::Result<IyonCore> {
    IyonCore::spawn()
}
