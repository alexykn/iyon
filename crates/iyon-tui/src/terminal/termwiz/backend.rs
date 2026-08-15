use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result, anyhow};
use crossterm::event::{Event, EventStream};
use futures_util::{FutureExt, StreamExt};
use tokio::sync::oneshot;

use crate::{
    backend::NativeHistorySink,
    geometry::Size,
    physical::PhysicalRow,
    scene::PreparedSceneFrame,
    terminal::{PresentReceipt, TerminalBackend, TerminalEvent},
};

use super::worker::{Startup, TerminalCommand};

pub(crate) struct TermwizBackend {
    commands: Sender<TerminalCommand>,
    events: EventStream,
    size: Size,
    worker: Option<JoinHandle<()>>,
    restored: bool,
}

impl TermwizBackend {
    pub(crate) fn enter() -> Result<Self> {
        let (commands, command_receiver) = mpsc::channel();
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("iyon-terminal".to_string())
            .spawn(move || super::worker::run(command_receiver, startup_sender))
            .context("spawn terminal worker")?;

        let startup = match startup_receiver.recv() {
            Ok(startup) => startup,
            Err(error) => {
                let _ = worker.join();
                return Err(anyhow!(error)).context("terminal worker startup failed");
            }
        }?;

        Ok(Self::from_startup(
            commands,
            EventStream::new(),
            worker,
            startup,
        ))
    }

    fn from_startup(
        commands: Sender<TerminalCommand>,
        events: EventStream,
        worker: JoinHandle<()>,
        startup: Startup,
    ) -> Self {
        Self {
            commands,
            events,
            size: startup.size,
            worker: Some(worker),
            restored: false,
        }
    }

    fn send<T>(&self, command: TerminalCommand, receiver: mpsc::Receiver<Result<T>>) -> Result<T> {
        self.commands
            .send(command)
            .context("terminal worker stopped")?;
        receiver.recv().context("terminal worker reply lost")?
    }
}

impl NativeHistorySink for TermwizBackend {
    type Error = anyhow::Error;

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
        let (reply, receiver) = mpsc::channel();
        self.send(
            TerminalCommand::InsertHistory {
                rows: rows.to_vec(),
                reply,
            },
            receiver,
        )
    }
}

impl TerminalBackend for TermwizBackend {
    async fn next_event(&mut self) -> Result<TerminalEvent> {
        loop {
            let event = self
                .events
                .next()
                .await
                .context("terminal event stream closed")??;
            if let Some(event) = self.map_event(event) {
                return Ok(event);
            }
        }
    }

    fn try_next_event(&mut self) -> Result<Option<TerminalEvent>> {
        loop {
            let event = match self.events.next().now_or_never() {
                None => return Ok(None),
                Some(None) => return Err(anyhow!("terminal event stream closed")),
                Some(Some(event)) => event.context("read terminal event")?,
            };
            if let Some(event) = self.map_event(event) {
                return Ok(Some(event));
            }
        }
    }

    fn viewport(&mut self) -> Result<Size> {
        Ok(self.size)
    }

    fn begin_frame(&mut self, frame: &PreparedSceneFrame) -> Result<PresentReceipt> {
        let (reply, receiver) = oneshot::channel();
        let desired = super::lower::desired_surface(frame);
        self.commands
            .send(TerminalCommand::Present { desired, reply })
            .context("terminal worker stopped")?;
        Ok(receiver)
    }

    fn position_after_final_frame(&mut self) -> Result<()> {
        let (reply, receiver) = mpsc::channel();
        self.send(TerminalCommand::PositionAfterFinalFrame { reply }, receiver)
    }

    fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        let (reply, receiver) = mpsc::channel();
        let result = self.send(TerminalCommand::Restore { reply }, receiver);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        result
    }
}

impl TermwizBackend {
    fn map_event(&mut self, event: Event) -> Option<TerminalEvent> {
        if let Event::Resize(width, height) = event {
            self.size = Size::new(width, height);
            return Some(TerminalEvent::Resize);
        }
        crate::terminal::crossterm::map_event(event)
    }
}

impl Drop for TermwizBackend {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.restore();
        }
    }
}
