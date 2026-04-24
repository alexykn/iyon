use anyhow::Result;
use tokio::{runtime::Runtime, sync::mpsc};

use crate::{CoreCommand, CoreEvent, runtime};

pub struct IyonCore {
    command_tx: mpsc::Sender<CoreCommand>,
    event_rx: mpsc::Receiver<CoreEvent>,
    _runtime: Runtime,
}

impl IyonCore {
    pub fn spawn() -> Result<Self> {
        let runtime = Runtime::new()?;
        let (command_tx, command_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(1024);

        runtime.spawn(runtime::run(command_rx, event_tx));

        Ok(Self {
            command_tx,
            event_rx,
            _runtime: runtime,
        })
    }

    pub fn try_send(&self, command: CoreCommand) -> Result<()> {
        self.command_tx.try_send(command)?;
        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<CoreEvent> {
        let mut events = Vec::new();
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => events.push(event),
                Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
                    return events;
                }
            }
        }
    }
}
