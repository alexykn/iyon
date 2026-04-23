use anyhow::Result;
use tokio::sync::mpsc::{Receiver, Sender, error::TryRecvError};

use crate::core::controller::FrontendAction;

#[derive(Debug)]
pub(crate) enum BackendCommand {
    SubmitTurn { text: String },
    CancelActiveTurn,
}

#[derive(Debug)]
pub(crate) enum FrontendEvent {
    TurnStarted,
    AssistantDelta { text: String },
    TurnFinished,
    TurnFailed { message: String },
}

#[derive(Debug, Default)]
pub(crate) struct BackendEventHandler {
    command_tx: Option<Sender<BackendCommand>>,
    event_rx: Option<Receiver<FrontendEvent>>,
    in_flight: bool,
}

impl BackendEventHandler {
    pub(crate) fn new(
        command_tx: Sender<BackendCommand>,
        event_rx: Receiver<FrontendEvent>,
    ) -> Self {
        Self {
            command_tx: Some(command_tx),
            event_rx: Some(event_rx),
            in_flight: false,
        }
    }

    pub(crate) fn try_submit_turn(&mut self, text: String) -> Result<()> {
        let Some(command_tx) = self.command_tx.as_ref() else {
            return Ok(());
        };

        command_tx.try_send(BackendCommand::SubmitTurn { text })?;
        self.in_flight = true;
        Ok(())
    }

    pub(crate) fn try_cancel_active_turn(&mut self) -> Result<()> {
        let Some(command_tx) = self.command_tx.as_ref() else {
            return Ok(());
        };

        command_tx.try_send(BackendCommand::CancelActiveTurn)?;
        Ok(())
    }

    pub(crate) fn has_in_flight_turn(&self) -> bool {
        self.in_flight
    }

    pub(crate) fn drain_actions(&mut self) -> Result<Vec<FrontendAction>> {
        let Some(event_rx) = self.event_rx.as_mut() else {
            return Ok(Vec::new());
        };

        let mut actions = Vec::new();
        loop {
            match event_rx.try_recv() {
                Ok(FrontendEvent::TurnStarted) => {
                    self.in_flight = true;
                    actions.push(FrontendAction::BackendTurnStarted);
                }
                Ok(FrontendEvent::AssistantDelta { text }) => {
                    actions.push(FrontendAction::AssistantDelta { text });
                }
                Ok(FrontendEvent::TurnFinished) => {
                    self.in_flight = false;
                    actions.push(FrontendAction::BackendTurnFinished);
                }
                Ok(FrontendEvent::TurnFailed { message }) => {
                    self.in_flight = false;
                    actions.push(FrontendAction::BackendTurnFailed { message });
                }
                Err(TryRecvError::Empty) => return Ok(actions),
                Err(TryRecvError::Disconnected) => {
                    self.in_flight = false;
                    return Ok(actions);
                }
            }
        }
    }
}
