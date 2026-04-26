use std::{collections::HashMap, fmt};

use anyhow::Result;
use iyon_core::{
    CoreCommand, CoreCommandSender, CoreEvent, CoreEventReceiver, IyonCore, MessageDelta,
    MessageRole,
};

#[derive(Debug)]
pub(crate) enum BackendCommand {
    SubmitTurn { text: String },
    CancelActiveTurn,
}

#[derive(Debug)]
pub(crate) enum FrontendEvent {
    TurnStarted,
    UserMessage { text: String },
    AssistantDelta { text: String },
    TurnFinished,
    TurnFailed { message: String },
    TurnCancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubmitTurnResult {
    Sent,
    NoBackendAttached,
}

pub(crate) struct BackendEventHandler {
    command_tx: Option<CoreCommandSender>,
    event_rx: Option<CoreEventReceiver>,
    in_flight: bool,
    message_roles: HashMap<u64, MessageRole>,
}

impl fmt::Debug for BackendEventHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackendEventHandler")
            .field("core_attached", &self.command_tx.is_some())
            .field("event_rx_attached", &self.event_rx.is_some())
            .field("in_flight", &self.in_flight)
            .field("tracked_messages", &self.message_roles.len())
            .finish()
    }
}

impl Default for BackendEventHandler {
    fn default() -> Self {
        let core = if tokio::runtime::Handle::try_current().is_ok() {
            Some(IyonCore::spawn_default_on_current_runtime())
        } else {
            iyon_core::spawn().ok()
        };

        match core {
            Some(core) => Self::new(core),
            None => Self {
                command_tx: None,
                event_rx: None,
                in_flight: false,
                message_roles: HashMap::new(),
            },
        }
    }
}

impl BackendEventHandler {
    pub(crate) fn new(core: IyonCore) -> Self {
        let (command_tx, event_rx) = core.split();
        Self {
            command_tx: Some(command_tx),
            event_rx: Some(event_rx),
            in_flight: false,
            message_roles: HashMap::new(),
        }
    }

    pub(crate) fn try_submit_turn(&mut self, text: String) -> Result<SubmitTurnResult> {
        let Some(command_tx) = self.command_tx.as_ref() else {
            return Ok(SubmitTurnResult::NoBackendAttached);
        };

        command_tx.try_send(CoreCommand::SubmitTurn { text })?;
        self.in_flight = true;
        Ok(SubmitTurnResult::Sent)
    }

    pub(crate) fn try_cancel_active_turn(&mut self) -> Result<()> {
        let Some(command_tx) = self.command_tx.as_ref() else {
            return Ok(());
        };

        command_tx.try_send(CoreCommand::CancelActiveTurn)?;
        Ok(())
    }

    pub(crate) fn has_in_flight_turn(&self) -> bool {
        self.in_flight
    }

    pub(crate) fn take_event_receiver(&mut self) -> Option<CoreEventReceiver> {
        self.event_rx.take()
    }

    pub(crate) fn drain_events(&mut self) -> Result<Vec<FrontendEvent>> {
        let Some(event_rx) = self.event_rx.as_mut() else {
            return Ok(Vec::new());
        };

        let events = event_rx.drain_events();
        Ok(self.map_core_events(events))
    }

    pub(crate) fn map_core_events(
        &mut self,
        events: impl IntoIterator<Item = CoreEvent>,
    ) -> Vec<FrontendEvent> {
        events
            .into_iter()
            .filter_map(|event| self.map_core_event(event))
            .collect()
    }

    pub(crate) fn map_core_event(&mut self, event: CoreEvent) -> Option<FrontendEvent> {
        match event {
            CoreEvent::AgentStarted | CoreEvent::AgentFinished => None,
            CoreEvent::TurnStarted { .. } => {
                self.in_flight = true;
                Some(FrontendEvent::TurnStarted)
            }
            CoreEvent::MessageStarted {
                message_id, role, ..
            } => {
                self.message_roles.insert(message_id, role);
                None
            }
            CoreEvent::MessageDelta {
                message_id,
                delta: MessageDelta::Text(text),
                ..
            } => match self.message_roles.get(&message_id).copied() {
                Some(MessageRole::User) => Some(FrontendEvent::UserMessage { text }),
                Some(MessageRole::Assistant) => Some(FrontendEvent::AssistantDelta { text }),
                Some(MessageRole::ToolResult | MessageRole::Status) | None => None,
            },
            CoreEvent::MessageDelta {
                delta: MessageDelta::ToolCall { .. },
                ..
            } => None,
            CoreEvent::MessageFinished { message_id, .. } => {
                self.message_roles.remove(&message_id);
                None
            }
            CoreEvent::ToolCallStarted { .. }
            | CoreEvent::ToolCallFinished { .. }
            | CoreEvent::ToolCallUpdated { .. }
            | CoreEvent::ToolApprovalRequested { .. }
            | CoreEvent::ToolApprovalResolved { .. } => None,
            CoreEvent::TurnFinished { .. } => {
                self.in_flight = false;
                Some(FrontendEvent::TurnFinished)
            }
            CoreEvent::TurnFailed { message, .. } => {
                self.in_flight = false;
                Some(FrontendEvent::TurnFailed { message })
            }
            CoreEvent::TurnCancelled { .. } => {
                self.in_flight = false;
                self.message_roles.clear();
                Some(FrontendEvent::TurnCancelled)
            }
        }
    }
}
