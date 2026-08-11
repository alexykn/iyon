use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result, anyhow};
use termwiz::terminal::TerminalWaker;

use crate::{
    backend::NativeHistorySink,
    geometry::Size,
    physical::PhysicalRow,
    scene::PreparedSceneFrame,
    terminal::{TerminalBackend, TerminalEvent},
};

use super::worker::{Startup, TerminalCommand};

pub(crate) struct TermwizBackend {
    commands: Sender<TerminalCommand>,
    events: tokio::sync::mpsc::UnboundedReceiver<Result<TerminalEvent>>,
    waker: TerminalWaker,
    worker: Option<JoinHandle<()>>,
    restored: bool,
}

impl TermwizBackend {
    pub(crate) fn enter() -> Result<Self> {
        let (commands, command_receiver) = mpsc::channel();
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let (events_sender, events) = tokio::sync::mpsc::unbounded_channel();
        let worker = thread::Builder::new()
            .name("iyon-terminal".to_string())
            .spawn(move || super::worker::run(command_receiver, events_sender, startup_sender))
            .context("spawn terminal worker")?;

        let startup = match startup_receiver.recv() {
            Ok(startup) => startup,
            Err(error) => {
                let _ = worker.join();
                return Err(anyhow!(error)).context("terminal worker startup failed");
            }
        }?;

        Ok(Self::from_startup(commands, events, worker, startup))
    }

    fn from_startup(
        commands: Sender<TerminalCommand>,
        events: tokio::sync::mpsc::UnboundedReceiver<Result<TerminalEvent>>,
        worker: JoinHandle<()>,
        startup: Startup,
    ) -> Self {
        Self {
            commands,
            events,
            waker: startup.waker,
            worker: Some(worker),
            restored: false,
        }
    }

    fn send<T>(&self, command: TerminalCommand, receiver: mpsc::Receiver<Result<T>>) -> Result<T> {
        self.commands
            .send(command)
            .context("terminal worker stopped")?;
        self.waker.wake().context("wake terminal worker")?;
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
        self.events
            .recv()
            .await
            .ok_or_else(|| anyhow!("terminal worker event stream closed"))?
    }

    fn try_next_event(&mut self) -> Result<Option<TerminalEvent>> {
        match self.events.try_recv() {
            Ok(event) => event.map(Some),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => Ok(None),
        }
    }

    fn viewport(&mut self) -> Result<Size> {
        let (reply, receiver) = mpsc::channel();
        self.send(TerminalCommand::Viewport { reply }, receiver)
    }

    fn draw_frame(&mut self, frame: &PreparedSceneFrame) -> Result<()> {
        let (reply, receiver) = mpsc::channel();
        let desired = super::lower::desired_surface(frame);
        self.send(TerminalCommand::Present { desired, reply }, receiver)
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

impl Drop for TermwizBackend {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.restore();
        }
    }
}
