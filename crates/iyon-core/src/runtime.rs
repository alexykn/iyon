use std::{path::PathBuf, sync::Arc, time::SystemTime};

use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::{
    CoreCommand, CoreEvent, MessageDelta, MessageRole,
    agent::{
        control::AgentLoopControl,
        r#loop::{AgentLoopInput, TurnOutcome, run_agent_loop},
        state::AgentState,
        transcript::AgentMessage,
    },
    fs::{FsPermissions, Workspace},
    ids::{ApprovalId, MessageId, SessionId, TurnId},
    session::state::SessionState,
    tools::ToolRegistry,
};
use iyon_api::{ContentBlock, ModelApi, StopReason};

struct RuntimeState {
    session: SessionState,
    agent: AgentState,
    next_turn_id: u64,
    next_message_id: u64,
    next_approval_id: u64,
    active_turn: Option<ActiveTurn>,
    tools: ToolRegistry,
    #[allow(dead_code)]
    workspace: Workspace,
}

struct ActiveTurn {
    turn_id: TurnId,
    handle: JoinHandle<()>,
    control_tx: mpsc::Sender<AgentLoopControl>,
    cancellation: CancellationToken,
}

#[derive(Debug)]
enum RuntimeEvent {
    TurnTaskFinished {
        turn_id: TurnId,
        outcome: TurnOutcome,
    },
}

impl RuntimeState {
    fn new(cwd: PathBuf) -> Self {
        let workspace = Workspace::new(cwd.clone(), FsPermissions::default());
        let mut tools = ToolRegistry::new();
        tools
            .register_builtin_defaults()
            .expect("builtin tool registration should be valid");

        Self {
            session: SessionState::new(SessionId(1), cwd),
            agent: AgentState::default(),
            next_turn_id: 1,
            next_message_id: 1,
            next_approval_id: 1,
            active_turn: None,
            tools,
            workspace,
        }
    }

    fn next_turn_id(&mut self) -> TurnId {
        let id = TurnId(self.next_turn_id);
        self.next_turn_id = self.next_turn_id.saturating_add(1);
        id
    }

    fn next_message_id(&mut self) -> MessageId {
        let id = MessageId(self.next_message_id);
        self.next_message_id = self.next_message_id.saturating_add(1);
        id
    }

    fn push_user_message(&mut self, text: String) -> MessageId {
        let id = self.next_message_id();
        self.session.messages.push(AgentMessage::User {
            id,
            content: vec![ContentBlock::Text { text }],
            timestamp: SystemTime::now(),
        });
        id
    }

    #[allow(dead_code)]
    fn push_assistant_text_with_id(
        &mut self,
        id: MessageId,
        text: String,
        stop_reason: Option<StopReason>,
    ) {
        self.session.messages.push(AgentMessage::Assistant {
            id,
            content: vec![ContentBlock::Text { text }],
            usage: None,
            stop_reason,
            timestamp: SystemTime::now(),
        });
    }

    fn commit_turn_messages(&mut self, messages: Vec<AgentMessage>, next_message_id: u64) {
        self.session.messages.extend(messages);
        self.next_message_id = next_message_id;
    }

    #[cfg(test)]
    fn push_assistant_text(&mut self, text: String, stop_reason: Option<StopReason>) -> MessageId {
        let id = self.next_message_id();
        self.push_assistant_text_with_id(id, text, stop_reason);
        id
    }

    fn mark_turn_started(&mut self, turn_id: TurnId, assistant_message_id: MessageId) {
        self.agent.is_running = true;
        self.agent.active_turn_id = Some(turn_id);
        self.agent.active_message_id = Some(assistant_message_id);
        self.agent.error_message = None;
    }

    fn mark_turn_finished(&mut self) {
        self.agent.is_running = false;
        self.agent.active_turn_id = None;
        self.agent.active_message_id = None;
    }
}

pub async fn run(
    mut command_rx: mpsc::Receiver<CoreCommand>,
    event_tx: mpsc::Sender<CoreEvent>,
    model: Arc<dyn ModelApi>,
) {
    let (runtime_event_tx, mut runtime_event_rx) = mpsc::channel(32);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut state = RuntimeState::new(cwd);

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    cancel_active_turn(&mut state, &event_tx).await;
                    break;
                };

                match command {
                    CoreCommand::SubmitTurn { text } => {
                        cancel_active_turn(&mut state, &event_tx).await;

                        let turn_id = state.next_turn_id();
                        let user_message_id = state.push_user_message(text.clone());
                        let assistant_message_id = MessageId(state.next_message_id);
                        state.mark_turn_started(turn_id, assistant_message_id);

                        emit_turn_start_and_user_message(
                            &event_tx,
                            turn_id,
                            user_message_id,
                            text,
                        )
                        .await;

                        let model = Arc::clone(&model);
                        let event_tx = event_tx.clone();
                        let runtime_event_tx = runtime_event_tx.clone();
                        let session = state.session.clone();
                        let tools = state.tools.clone();
                        let workspace = state.workspace.clone();
                        let next_message_id = state.next_message_id;
                        let next_approval_id = state.next_approval_id;
                        let (control_tx, control_rx) = mpsc::channel(8);
                        let cancellation = CancellationToken::new();
                        let task_cancellation = cancellation.clone();
                        let handle = tokio::spawn(async move {
                            let outcome = run_agent_loop(AgentLoopInput {
                                turn_id,
                                next_message_id,
                                next_approval_id,
                                session,
                                model,
                                tools,
                                workspace,
                                event_tx,
                                control_rx,
                                cancellation: task_cancellation,
                            })
                            .await;
                            let _ = runtime_event_tx
                                .send(RuntimeEvent::TurnTaskFinished { turn_id, outcome })
                                .await;
                        });
                        state.active_turn = Some(ActiveTurn {
                            turn_id,
                            handle,
                            control_tx,
                            cancellation,
                        });
                    }
                    CoreCommand::CancelActiveTurn => {
                        cancel_active_turn(&mut state, &event_tx).await;
                    }
                    CoreCommand::ApproveToolCall { approval_id } => {
                        forward_active_control(
                            &state,
                            AgentLoopControl::ApproveToolCall {
                                approval_id: ApprovalId(approval_id),
                            },
                        )
                        .await;
                    }
                    CoreCommand::RejectToolCall { approval_id, reason } => {
                        forward_active_control(
                            &state,
                            AgentLoopControl::RejectToolCall {
                                approval_id: ApprovalId(approval_id),
                                reason,
                            },
                        )
                        .await;
                    }
                    CoreCommand::Shutdown => {
                        cancel_active_turn(&mut state, &event_tx).await;
                        break;
                    }
                }
            }
            runtime_event = runtime_event_rx.recv() => {
                let Some(RuntimeEvent::TurnTaskFinished { turn_id, outcome }) = runtime_event else {
                    continue;
                };

                if !state
                    .active_turn
                    .as_ref()
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    continue;
                }

                state.active_turn = None;
                state.mark_turn_finished();

                match outcome {
                    TurnOutcome::Completed {
                        messages,
                        next_message_id,
                        next_approval_id,
                    } => {
                        state.commit_turn_messages(messages, next_message_id);
                        state.next_approval_id = next_approval_id;
                    }
                    TurnOutcome::Cancelled => {
                        let _ = event_tx
                            .send(CoreEvent::TurnCancelled { turn_id: turn_id.0 })
                            .await;
                        let _ = event_tx.send(CoreEvent::AgentFinished).await;
                    }
                    TurnOutcome::Failed { error } => {
                        let _ = event_tx
                            .send(CoreEvent::TurnFailed {
                                turn_id: turn_id.0,
                                message: error,
                            })
                            .await;
                        let _ = event_tx.send(CoreEvent::AgentFinished).await;
                    }
                }
            }
        }
    }
}

async fn emit_turn_start_and_user_message(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    user_message_id: MessageId,
    text: String,
) {
    let _ = event_tx.send(CoreEvent::AgentStarted).await;
    let _ = event_tx
        .send(CoreEvent::TurnStarted { turn_id: turn_id.0 })
        .await;
    let _ = event_tx
        .send(CoreEvent::MessageStarted {
            turn_id: turn_id.0,
            message_id: user_message_id.0,
            role: MessageRole::User,
        })
        .await;
    let _ = event_tx
        .send(CoreEvent::MessageDelta {
            turn_id: turn_id.0,
            message_id: user_message_id.0,
            delta: MessageDelta::Text(text),
        })
        .await;
    let _ = event_tx
        .send(CoreEvent::MessageFinished {
            turn_id: turn_id.0,
            message_id: user_message_id.0,
        })
        .await;
}

async fn cancel_active_turn(state: &mut RuntimeState, event_tx: &mpsc::Sender<CoreEvent>) {
    let Some(active) = state.active_turn.take() else {
        return;
    };

    active.cancellation.cancel();
    let _ = active.control_tx.try_send(AgentLoopControl::Cancel);
    active.handle.abort();
    state.mark_turn_finished();
    let _ = event_tx
        .send(CoreEvent::TurnCancelled {
            turn_id: active.turn_id.0,
        })
        .await;
    let _ = event_tx.send(CoreEvent::AgentFinished).await;
}

async fn forward_active_control(state: &RuntimeState, control: AgentLoopControl) {
    let Some(active) = state.active_turn.as_ref() else {
        return;
    };
    let _ = active.control_tx.send(control).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_state_allocates_monotonic_ids() {
        let mut state = RuntimeState::new(PathBuf::from("/tmp"));

        assert_eq!(state.next_turn_id(), TurnId(1));
        assert_eq!(state.next_turn_id(), TurnId(2));
        assert_eq!(state.next_message_id(), MessageId(1));
        assert_eq!(state.next_message_id(), MessageId(2));
    }

    #[test]
    fn push_user_message_allocates_id_and_appends_transcript_entry() {
        let mut state = RuntimeState::new(PathBuf::from("/tmp"));

        let id = state.push_user_message("hello".to_string());

        assert_eq!(id, MessageId(1));
        assert_eq!(state.session.messages.len(), 1);
        let AgentMessage::User { id, content, .. } = &state.session.messages[0] else {
            panic!("expected user message");
        };
        assert_eq!(*id, MessageId(1));
        assert!(matches!(
            &content[..],
            [ContentBlock::Text { text }] if text == "hello"
        ));
    }

    #[test]
    fn push_assistant_text_allocates_id_and_appends_transcript_entry() {
        let mut state = RuntimeState::new(PathBuf::from("/tmp"));

        let id = state.push_assistant_text("hi".to_string(), Some(StopReason::Stop));

        assert_eq!(id, MessageId(1));
        assert_eq!(state.session.messages.len(), 1);
        let AgentMessage::Assistant {
            id,
            content,
            stop_reason,
            ..
        } = &state.session.messages[0]
        else {
            panic!("expected assistant message");
        };
        assert_eq!(*id, MessageId(1));
        assert_eq!(*stop_reason, Some(StopReason::Stop));
        assert!(matches!(
            &content[..],
            [ContentBlock::Text { text }] if text == "hi"
        ));
    }

    #[test]
    fn push_assistant_text_with_id_uses_allocated_message_id() {
        let mut state = RuntimeState::new(PathBuf::from("/tmp"));
        let id = state.next_message_id();

        state.push_assistant_text_with_id(id, "hi".to_string(), Some(StopReason::Stop));

        let AgentMessage::Assistant { id: actual, .. } = &state.session.messages[0] else {
            panic!("expected assistant message");
        };
        assert_eq!(*actual, id);
    }
}
