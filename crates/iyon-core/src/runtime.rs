use std::collections::VecDeque;
use std::{path::PathBuf, sync::Arc, time::SystemTime};
use tokio::sync::Mutex;

use iyon_api::{ContentBlock, ModelApi, ReasoningLevel, StopReason};
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
    session::state::{ModelSelection, SessionState},
    tools::{FileMutationQueue, ToolHookSet, ToolRegistry},
};

struct RuntimeState {
    session: SessionState,
    agent: AgentState,
    next_turn_id: u64,
    next_message_id: u64,
    next_approval_id: u64,
    active_turn: Option<ActiveTurn>,
    /// Persistent steering queue shared with the running loop(s). Survives turn
    /// lifecycle, so queued messages are not lost on interrupt: on the next fresh
    /// submission they are drained and delivered before the new message.
    steering: Arc<Mutex<VecDeque<String>>>,
    tools: ToolRegistry,
    tool_hooks: ToolHookSet,
    #[allow(dead_code)]
    workspace: Workspace,
}

struct ActiveTurn {
    turn_id: TurnId,
    handle: JoinHandle<()>,
    control_tx: mpsc::Sender<AgentLoopControl>,
    cancellation: CancellationToken,
}

impl ActiveTurn {
    /// True while this turn is genuinely running and has not been signalled to stop —
    /// i.e. it can still accept and act on queued steering messages. A turn that was
    /// interrupted (cancellation requested) or has already finished cannot steer, so a
    /// new submission restarts the agent (and drains the queue) instead.
    ///
    /// This is derived from the turn's real state (its cancellation token + task handle),
    /// not a separate bookkeeping flag, so it always reflects whether the loop can act.
    fn is_steerable(&self) -> bool {
        !self.cancellation.is_cancelled() && !self.handle.is_finished()
    }
}

#[derive(Debug)]
enum RuntimeEvent {
    TurnTaskFinished {
        turn_id: TurnId,
        outcome: TurnOutcome,
    },
}

#[derive(Default)]
pub(crate) struct RuntimeConfig {
    pub tool_hooks: ToolHookSet,
    pub selection: ModelSelection,
}

impl RuntimeState {
    fn new(cwd: PathBuf, config: RuntimeConfig) -> Self {
        let workspace = Workspace::new(cwd.clone(), FsPermissions::default());
        let mutation_queue = FileMutationQueue::default();
        let mut tools = ToolRegistry::new();
        tools
            .register_builtin_defaults(mutation_queue)
            .expect("builtin tool registration should be valid");

        let mut session = SessionState::new(SessionId(1), cwd);
        session.model = config.selection;

        Self {
            session,
            agent: AgentState::default(),
            next_turn_id: 1,
            next_message_id: 1,
            next_approval_id: 1,
            active_turn: None,
            steering: Arc::new(Mutex::new(VecDeque::new())),
            tools,
            tool_hooks: config.tool_hooks,
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
    config: RuntimeConfig,
) {
    let (runtime_event_tx, mut runtime_event_rx) = mpsc::channel(32);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut state = RuntimeState::new(cwd, config);

    // Surface the initial provider/model/effort so the TUI status bar can render
    // immediately on first drain, before any turn.
    emit_config_changed(&event_tx, &state).await;

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else {
                    cancel_active_turn(&mut state, &event_tx).await;
                    break;
                };

                match command {
                    CoreCommand::SubmitTurn { text } => {
                        // A new submission routes to the queue (as a steer) only while the
                        // active turn is alive and can still act on queued messages.
                        // Otherwise it restarts the agent: any ending turn is folded in
                        // first, then everything still queued is drained into the new turn.
                        if state.active_turn.as_ref().is_some_and(ActiveTurn::is_steerable) {
                            // The steering lock is only held for the VecDeque push — never
                            // across an await — so the critical section stays bounded.
                            state.steering.lock().await.push_back(text.clone());
                            let _ = event_tx.send(CoreEvent::SteerQueued { text }).await;
                            continue;
                        }

                        // Restart: fold in the previous (cancelling / finished) turn so its
                        // outcome is applied (e.g. a completed reply is kept) and only one
                        // agent loop ever runs. This is what makes the queue drain reliably
                        // on restart after an interrupt, regardless of timing.
                        settle_active_turn(&mut state, &mut runtime_event_rx, &event_tx).await;

                        let turn_id = state.next_turn_id();
                        let _ = event_tx.send(CoreEvent::AgentStarted).await;
                        let _ = event_tx
                            .send(CoreEvent::TurnStarted { turn_id: turn_id.0 })
                            .await;

                        // Drain any steers queued from an interrupted/ended turn into this
                        // turn's context first (delivered as user messages) so the agent
                        // acts on them too, then deliver the new message itself. All user
                        // ids are allocated before the assistant id is reserved below.
                        let pending_steers: Vec<String> =
                            state.steering.lock().await.drain(..).collect();
                        for steer in pending_steers {
                            let id = state.next_message_id();
                            state.session.messages.push(AgentMessage::User {
                                id,
                                content: vec![ContentBlock::Text { text: steer.clone() }],
                                timestamp: SystemTime::now(),
                            });
                            emit_user_message(&event_tx, turn_id, id, &steer).await;
                        }
                        let user_message_id = state.push_user_message(text.clone());
                        emit_user_message(&event_tx, turn_id, user_message_id, &text).await;

                        let assistant_message_id = MessageId(state.next_message_id);
                        state.mark_turn_started(turn_id, assistant_message_id);

                        let model = Arc::clone(&model);
                        let event_tx = event_tx.clone();
                        let runtime_event_tx = runtime_event_tx.clone();
                        let session = state.session.clone();
                        let tools = state.tools.clone();
                        let workspace = state.workspace.clone();
                        let tool_hooks = state.tool_hooks.snapshot();
                        let next_message_id = state.next_message_id;
                        let next_approval_id = state.next_approval_id;
                        let steering = Arc::clone(&state.steering);
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
                                tool_hooks,
                                event_tx,
                                control_rx,
                                steering,
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
                        request_interrupt(&state).await;
                    }
                    CoreCommand::CycleReasoningEffort => {
                        let provider = state.session.model.provider.as_str();
                        let next = ReasoningLevel::next_for(state.session.reasoning_effort, provider);
                        if next != state.session.reasoning_effort {
                            state.session.reasoning_effort = next;
                            emit_config_changed(&event_tx, &state).await;
                        }
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

                apply_turn_outcome(&mut state, &event_tx, turn_id, outcome).await;
            }
        }
    }
}

async fn emit_user_message(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    message_id: MessageId,
    text: &str,
) {
    let _ = event_tx
        .send(CoreEvent::MessageStarted {
            turn_id: turn_id.0,
            message_id: message_id.0,
            role: MessageRole::User,
        })
        .await;
    let _ = event_tx
        .send(CoreEvent::MessageDelta {
            turn_id: turn_id.0,
            message_id: message_id.0,
            delta: MessageDelta::Text(text.to_string()),
        })
        .await;
    let _ = event_tx
        .send(CoreEvent::MessageFinished {
            turn_id: turn_id.0,
            message_id: message_id.0,
        })
        .await;
}

/// Gracefully interrupts the running turn: signals the loop to stop and preserves its
/// partial output. Unlike a hard abort, this does NOT clear `active_turn` or emit
/// `TurnCancelled` — the loop task settles via `TurnTaskFinished` (or a later restart's
/// `settle_active_turn`) with a `Cancelled` outcome whose partial messages are committed.
async fn request_interrupt(state: &RuntimeState) {
    let Some(active) = state.active_turn.as_ref() else {
        return;
    };
    active.cancellation.cancel();
}

/// Applies a completed turn's outcome to the session and surfaces the terminal event
/// (for cancelled/failed turns). Shared by the normal completion path and the restart
/// settle path so both fold in outcomes identically.
async fn apply_turn_outcome(
    state: &mut RuntimeState,
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    outcome: TurnOutcome,
) {
    match outcome {
        TurnOutcome::Completed {
            messages,
            next_message_id,
            next_approval_id,
        } => {
            state.commit_turn_messages(messages, next_message_id);
            state.next_approval_id = next_approval_id;
        }
        TurnOutcome::Cancelled {
            messages,
            next_message_id,
            next_approval_id,
        } => {
            // Preserve the partial reply (and any steered user messages / tool results
            // produced before the interrupt) so a subsequent turn continues from a fully
            // consistent session.
            state.commit_turn_messages(messages, next_message_id);
            state.next_approval_id = next_approval_id;
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

/// Folds in a non-steerable active turn (interrupted or already finished) before a
/// restart: waits for its loop task to report completion, applies its outcome, and
/// clears the slot so a fresh turn can start. Keeping turn transitions serialized means
/// only one agent loop ever runs, and a reply that just completed is never lost to an
/// overlapping restart. This is what makes the queue drain reliably on restart after an
/// interrupt, regardless of exactly when the interrupt happened.
async fn settle_active_turn(
    state: &mut RuntimeState,
    runtime_event_rx: &mut mpsc::Receiver<RuntimeEvent>,
    event_tx: &mpsc::Sender<CoreEvent>,
) {
    let Some(old) = state.active_turn.take() else {
        return;
    };
    // Ensure it stops promptly (idempotent for an already-requested interrupt).
    old.cancellation.cancel();
    loop {
        match runtime_event_rx.recv().await {
            Some(RuntimeEvent::TurnTaskFinished { turn_id, outcome }) if turn_id == old.turn_id => {
                state.mark_turn_finished();
                apply_turn_outcome(state, event_tx, turn_id, outcome).await;
                return;
            }
            Some(_) => continue,
            None => return,
        }
    }
}

/// Hard-aborts the running turn, discarding partial output. Used only on teardown
/// (shutdown / command-channel close) where a prompt exit matters more than
/// preserving the in-flight reply, which is safely discarded.
async fn cancel_active_turn(state: &mut RuntimeState, event_tx: &mpsc::Sender<CoreEvent>) {
    let Some(active) = state.active_turn.take() else {
        return;
    };

    active.cancellation.cancel();
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

async fn emit_config_changed(event_tx: &mpsc::Sender<CoreEvent>, state: &RuntimeState) {
    let _ = event_tx
        .send(CoreEvent::ConfigChanged {
            provider: state.session.model.provider.clone(),
            model_id: state.session.model.model_id.clone(),
            reasoning_effort: state.session.reasoning_effort,
        })
        .await;
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Arc;
    use std::time::Duration;

    use futures_util::stream::unfold;
    use iyon_api::{
        ModelApi, ModelRequest, ModelStream, ModelStreamEvent, ModelStreamFuture, StopReason,
    };

    use super::{RuntimeConfig, run};
    use crate::{CoreCommand, CoreEvent, MessageDelta, MessageRole};

    /// Yields a pre-queued scripted stream per `stream()` call, so one `run()` can be
    /// driven across multiple turns. Counts stream starts so we can observe restarts.
    struct QueueModel {
        streams: Arc<std::sync::Mutex<VecDeque<tokio::sync::mpsc::Receiver<ModelStreamEvent>>>>,
        started: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ModelApi for QueueModel {
        fn stream(&self, _request: ModelRequest) -> ModelStreamFuture<'_> {
            let streams = Arc::clone(&self.streams);
            let started = Arc::clone(&self.started);
            Box::pin(async move {
                let rx = streams
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("a scripted stream should be queued for each model turn");
                started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let stream: ModelStream = Box::pin(unfold(rx, |mut rx| {
                    async move { rx.recv().await.map(|event| (Ok(event), rx)) }
                }));
                Ok(stream)
            })
        }
    }

    fn queued(
        n: usize,
    ) -> (
        Arc<QueueModel>,
        Vec<tokio::sync::mpsc::Sender<ModelStreamEvent>>,
    ) {
        let mut streams = VecDeque::new();
        let mut senders = Vec::new();
        for _ in 0..n {
            let (tx, rx) = tokio::sync::mpsc::channel(16);
            streams.push_back(rx);
            senders.push(tx);
        }
        (
            Arc::new(QueueModel {
                streams: Arc::new(std::sync::Mutex::new(streams)),
                started: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }),
            senders,
        )
    }

    /// Consume events up to (and including) an assistant text delta containing `needle`,
    /// filling a `message_id -> role` map so user/assistant text can be told apart.
    async fn wait_for_text(
        event_rx: &mut tokio::sync::mpsc::Receiver<CoreEvent>,
        roles: &mut HashMap<u64, MessageRole>,
        needle: &str,
    ) {
        loop {
            let Some(event) = event_rx.recv().await else {
                panic!("event stream closed before {needle:?} observed");
            };
            match event {
                CoreEvent::MessageStarted { message_id, role, .. } => {
                    roles.insert(message_id, role);
                }
                CoreEvent::MessageDelta {
                    delta: MessageDelta::Text(text),
                    ..
                } => {
                    if text.contains(needle) {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// An interrupt-then-restart must drain the steering queue into the fresh turn: a
    /// message queued before the interrupt is folded into the user's "continue" message
    /// rather than being left stuck in the queue (which happened when a submit landed on
    /// the dying turn's steer path instead of a restart).
    #[tokio::test]
    async fn interrupt_restart_drains_queued_steers_into_continue() {
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(16);
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
        let (model, senders) = queued(2);

        let runner = tokio::spawn(run(
            command_rx,
            event_tx.clone(),
            model.clone(),
            RuntimeConfig::default(),
        ));
        let mut roles: HashMap<u64, MessageRole> = HashMap::new();

        // Turn 1: user starts; the model streams a reply.
        command_tx
            .send(CoreCommand::SubmitTurn {
                text: "start".to_string(),
            })
            .await
            .unwrap();
        senders[0].send(ModelStreamEvent::Started).await.unwrap();
        senders[0]
            .send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "thinking".to_string(),
            })
            .await
            .unwrap();
        wait_for_text(&mut event_rx, &mut roles, "thinking").await;

        // While turn 1 is alive & streaming, a message is steered (queued).
        command_tx
            .send(CoreCommand::SubmitTurn {
                text: "queued-stuff".to_string(),
            })
            .await
            .unwrap();

        // Interrupt mid-thinking. Turn 1 is now cancelling (its token is cancelled) but
        // its loop has not necessarily settled yet — exactly the race that used to leave
        // "queued-stuff" stuck in the queue.
        command_tx.send(CoreCommand::CancelActiveTurn).await.unwrap();

        // The user continues by sending a message straight away — this must restart the
        // agent (not steer into the dying turn), settling turn 1 and draining the queue.
        command_tx
            .send(CoreCommand::SubmitTurn {
                text: "continue".to_string(),
            })
            .await
            .unwrap();

        // Drive turn 2 (the restart). Its stream buffers these events even if the loop
        // task has not popped the receiver yet.
        senders[1].send(ModelStreamEvent::Started).await.unwrap();
        senders[1]
            .send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "final".to_string(),
            })
            .await
            .unwrap();
        senders[1]
            .send(ModelStreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .await
            .unwrap();

        // A genuine restart must have occurred (turn 2's stream was requested).
        let started = std::sync::atomic::Ordering::SeqCst;
        for _ in 0..200 {
            if model.started.load(started) >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(model.started.load(started), 2, "expected a restart turn to start");

        // Close the command channel to bring run() to a clean stop, then drain all events
        // (tagging each message's role) to collect the user messages actually delivered.
        drop(command_tx);
        let _ = runner.await;

        let mut user_texts = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            match event {
                CoreEvent::MessageStarted { message_id, role, .. } => {
                    roles.insert(message_id, role);
                }
                CoreEvent::MessageDelta {
                    message_id,
                    delta: MessageDelta::Text(text),
                    ..
                } => {
                    if roles.get(&message_id) == Some(&MessageRole::User) {
                        user_texts.push(text);
                    }
                }
                _ => {}
            }
        }

        // The queued steer was folded into the restart: both "queued-stuff" and the user's
        // "continue" are delivered as user messages, so nothing sits stuck in the queue.
        assert!(
            user_texts.iter().any(|t| t.contains("queued-stuff")),
            "steer should be drained into the restart, got user texts: {user_texts:?}"
        );
        assert!(
            user_texts.iter().any(|t| t == "continue"),
            "continue should be delivered, got user texts: {user_texts:?}"
        );
    }
}

