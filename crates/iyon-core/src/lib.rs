mod command;
mod event;
mod handle;
mod runtime;

pub use command::CoreCommand;
pub use event::{CoreEvent, MessageDelta};
pub use handle::{CoreCommandSender, CoreEventReceiver, IyonCore};

pub fn spawn() -> anyhow::Result<IyonCore> {
    IyonCore::spawn()
}
