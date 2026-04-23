use anyhow::Result;
use tokio::sync::mpsc::{Receiver, Sender, error::TryRecvError};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubmitTurnResult {
    Sent,
    NoBackendAttached,
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

    pub(crate) fn try_submit_turn(&mut self, text: String) -> Result<SubmitTurnResult> {
        let Some(command_tx) = self.command_tx.as_ref() else {
            return Ok(SubmitTurnResult::NoBackendAttached);
        };

        command_tx.try_send(BackendCommand::SubmitTurn { text })?;
        self.in_flight = true;
        Ok(SubmitTurnResult::Sent)
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

    pub(crate) fn drain_events(&mut self) -> Result<Vec<FrontendEvent>> {
        let Some(event_rx) = self.event_rx.as_mut() else {
            return Ok(Vec::new());
        };

        let mut events = Vec::new();
        loop {
            match event_rx.try_recv() {
                Ok(FrontendEvent::TurnStarted) => {
                    self.in_flight = true;
                    events.push(FrontendEvent::TurnStarted);
                }
                Ok(FrontendEvent::AssistantDelta { text }) => {
                    events.push(FrontendEvent::AssistantDelta { text });
                }
                Ok(FrontendEvent::TurnFinished) => {
                    self.in_flight = false;
                    events.push(FrontendEvent::TurnFinished);
                }
                Ok(FrontendEvent::TurnFailed { message }) => {
                    self.in_flight = false;
                    events.push(FrontendEvent::TurnFailed { message });
                }
                Err(TryRecvError::Empty) => return Ok(events),
                Err(TryRecvError::Disconnected) => {
                    if self.in_flight {
                        self.in_flight = false;
                        events.push(FrontendEvent::TurnFailed {
                            message: "backend disconnected".to_string(),
                        });
                    }
                    return Ok(events);
                }
            }
        }
    }
}
