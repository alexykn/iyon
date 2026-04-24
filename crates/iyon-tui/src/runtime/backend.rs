use std::fmt;

use anyhow::Result;
use iyon_core::{CoreCommand, CoreEvent, IyonCore, MessageDelta};

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

pub(crate) struct BackendEventHandler {
    core: Option<IyonCore>,
    in_flight: bool,
}

impl fmt::Debug for BackendEventHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackendEventHandler")
            .field("core_attached", &self.core.is_some())
            .field("in_flight", &self.in_flight)
            .finish()
    }
}

impl Default for BackendEventHandler {
    fn default() -> Self {
        Self {
            core: iyon_core::spawn().ok(),
            in_flight: false,
        }
    }
}

impl BackendEventHandler {
    pub(crate) fn new(core: IyonCore) -> Self {
        Self {
            core: Some(core),
            in_flight: false,
        }
    }

    pub(crate) fn try_submit_turn(&mut self, text: String) -> Result<SubmitTurnResult> {
        let Some(core) = self.core.as_ref() else {
            return Ok(SubmitTurnResult::NoBackendAttached);
        };

        core.try_send(CoreCommand::SubmitTurn { text })?;
        self.in_flight = true;
        Ok(SubmitTurnResult::Sent)
    }

    pub(crate) fn try_cancel_active_turn(&mut self) -> Result<()> {
        let Some(core) = self.core.as_ref() else {
            return Ok(());
        };

        core.try_send(CoreCommand::CancelActiveTurn)?;
        Ok(())
    }

    pub(crate) fn has_in_flight_turn(&self) -> bool {
        self.in_flight
    }

    pub(crate) fn drain_events(&mut self) -> Result<Vec<FrontendEvent>> {
        let Some(core) = self.core.as_mut() else {
            return Ok(Vec::new());
        };

        let mut events = Vec::new();
        for event in core.drain_events() {
            match event {
                CoreEvent::TurnStarted { .. } => {
                    self.in_flight = true;
                    events.push(FrontendEvent::TurnStarted);
                }
                CoreEvent::MessageDelta {
                    delta: MessageDelta::Text(text),
                    ..
                } => events.push(FrontendEvent::AssistantDelta { text }),
                CoreEvent::TurnFinished { .. } => {
                    self.in_flight = false;
                    events.push(FrontendEvent::TurnFinished);
                }
                CoreEvent::TurnFailed { message, .. } => {
                    self.in_flight = false;
                    events.push(FrontendEvent::TurnFailed { message });
                }
            }
        }

        Ok(events)
    }
}
