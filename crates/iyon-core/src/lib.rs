mod command;
mod event;
mod handle;
mod runtime;

pub use command::CoreCommand;
pub use event::{CoreEvent, MessageDelta};
pub use handle::IyonCore;

pub fn spawn() -> anyhow::Result<IyonCore> {
    IyonCore::spawn()
}
