use std::sync::Arc;

use anyhow::Result;
use iyon_api::{MockModelApi, ModelApi};
use tokio::{runtime::Runtime, sync::mpsc};

use crate::{
    CoreCommand, CoreEvent,
    runtime::{self, RuntimeConfig},
    session::state::ModelSelection,
    tools::ToolHookSet,
};

pub struct IyonCore {
    command_tx: CoreCommandSender,
    event_rx: CoreEventReceiver,
}

#[derive(Clone)]
pub struct CoreCommandSender {
    command_tx: mpsc::Sender<CoreCommand>,
}

pub struct CoreEventReceiver {
    event_rx: mpsc::Receiver<CoreEvent>,
    _runtime: RuntimeGuard,
}

enum RuntimeGuard {
    Owned(Runtime),
    External,
}

impl IyonCore {
    pub fn spawn() -> Result<Self> {
        Self::spawn_with_owned_runtime(Arc::new(MockModelApi))
    }

    pub fn spawn_with_owned_runtime(model: Arc<dyn ModelApi>) -> Result<Self> {
        Self::spawn_with_owned_runtime_and_hooks(model, ToolHookSet::default())
    }

    pub fn spawn_with_owned_runtime_and_hooks(
        model: Arc<dyn ModelApi>,
        tool_hooks: ToolHookSet,
    ) -> Result<Self> {
        let runtime = Runtime::new()?;
        let (command_tx, command_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(1024);

        runtime.spawn(runtime::run(
            command_rx,
            event_tx,
            model,
            RuntimeConfig {
                tool_hooks,
                selection: ModelSelection::default(),
            },
        ));

        Ok(Self {
            command_tx: CoreCommandSender { command_tx },
            event_rx: CoreEventReceiver {
                event_rx,
                _runtime: RuntimeGuard::Owned(runtime),
            },
        })
    }

    #[must_use]
    pub fn spawn_default_on_current_runtime() -> Self {
        Self::spawn_on_current_runtime(Arc::new(MockModelApi))
    }

    pub fn spawn_on_current_runtime(model: Arc<dyn ModelApi>) -> Self {
        Self::spawn_on_current_runtime_and_hooks(model, ToolHookSet::default())
    }

    pub fn spawn_on_current_runtime_and_hooks(
        model: Arc<dyn ModelApi>,
        tool_hooks: ToolHookSet,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(1024);

        tokio::spawn(runtime::run(
            command_rx,
            event_tx,
            model,
            RuntimeConfig {
                tool_hooks,
                selection: ModelSelection::default(),
            },
        ));

        Self {
            command_tx: CoreCommandSender { command_tx },
            event_rx: CoreEventReceiver {
                event_rx,
                _runtime: RuntimeGuard::External,
            },
        }
    }

    pub fn split(self) -> (CoreCommandSender, CoreEventReceiver) {
        (self.command_tx, self.event_rx)
    }

    pub fn try_send(&self, command: CoreCommand) -> Result<()> {
        self.command_tx.try_send(command)
    }

    pub fn try_recv_event(&mut self) -> Option<CoreEvent> {
        self.event_rx.try_recv_event()
    }

    pub fn blocking_recv_event(&mut self) -> Option<CoreEvent> {
        self.event_rx.blocking_recv_event()
    }

    pub fn drain_events(&mut self) -> Vec<CoreEvent> {
        self.event_rx.drain_events()
    }
    pub fn spawn_on_current_runtime_with_selection_and_hooks(
        model: Arc<dyn ModelApi>,
        selection: ModelSelection,
        tool_hooks: ToolHookSet,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(1024);

        tokio::spawn(runtime::run(
            command_rx,
            event_tx,
            model,
            RuntimeConfig {
                tool_hooks,
                selection,
            },
        ));

        Self {
            command_tx: CoreCommandSender { command_tx },
            event_rx: CoreEventReceiver {
                event_rx,
                _runtime: RuntimeGuard::External,
            },
        }
    }
}

impl CoreCommandSender {
    pub fn try_send(&self, command: CoreCommand) -> Result<()> {
        self.command_tx.try_send(command)?;
        Ok(())
    }
}

impl Drop for CoreEventReceiver {
    fn drop(&mut self) {
        match &self._runtime {
            RuntimeGuard::Owned(runtime) => {
                let _ = runtime.handle();
            }
            RuntimeGuard::External => {}
        }
    }
}

impl CoreEventReceiver {
    /// Receives the next core event without blocking a synchronous thread.
    pub async fn recv_event(&mut self) -> Option<CoreEvent> {
        self.event_rx.recv().await
    }

    pub fn try_recv_event(&mut self) -> Option<CoreEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn blocking_recv_event(&mut self) -> Option<CoreEvent> {
        self.event_rx.blocking_recv()
    }

    pub fn drain_events(&mut self) -> Vec<CoreEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.try_recv_event() {
            events.push(event);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreEvent;

    #[tokio::test]
    async fn async_receive_reads_the_real_core_event_channel() {
        let core = IyonCore::spawn_default_on_current_runtime();
        let (_commands, mut events) = core.split();
        let event = events.recv_event().await.expect("initial core event");
        assert!(matches!(event, CoreEvent::ConfigChanged { .. }));
    }
}
