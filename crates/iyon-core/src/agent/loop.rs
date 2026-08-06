use std::collections::VecDeque;
use std::{path::PathBuf, sync::Arc, time::SystemTime};
use tokio::sync::Mutex;

use anyhow::{Context, Result, bail};
use iyon_api::{ContentBlock, ModelApi, StopReason};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreEvent, MessageDelta, MessageRole,
    agent::{
        control::AgentLoopControl,
        request::{RequestBuildContext, build_model_request},
        tool_call::ToolCallRequest,
        tool_execution::{
            ToolExecutionInput, ToolExecutionOutcome, execute_tool_call, invalid_tool_call_result,
        },
        transcript::AgentMessage,
        turn::{ModelTurnInput, ModelTurnOutcome, run_model_turn},
    },
    fs::Workspace,
    ids::{MessageId, TurnId},
    session::state::SessionState,
    tools::{ToolHookSnapshot, ToolRegistry},
};

const MAX_TOOL_CALLS_PER_MODEL_TURN: usize = 16;

enum LoopAction {
    ExecuteTools,
    Finish,
}

pub(crate) struct AgentLoopInput {
    pub turn_id: TurnId,
    pub next_message_id: u64,
    pub next_approval_id: u64,
    pub session: SessionState,
    pub model: Arc<dyn ModelApi>,
    pub tools: ToolRegistry,
    pub workspace: Workspace,
    pub tool_hooks: ToolHookSnapshot,
    pub event_tx: mpsc::Sender<CoreEvent>,
    pub control_rx: mpsc::Receiver<AgentLoopControl>,
    /// Shared, persistent steering queue (owned by the runtime so it survives turn
    /// lifecycle). The loop drains it at turn boundaries; it is never lost on interrupt.
    pub steering: Arc<Mutex<VecDeque<String>>>,
    pub cancellation: CancellationToken,
}

struct ExecuteToolRequestsInput<'a> {
    tool_calls: Vec<ToolCallRequest>,
    ids: &'a mut MessageIdAllocator,
    session: &'a mut SessionState,
    produced_messages: &'a mut Vec<AgentMessage>,
    turn_id: TurnId,
    workspace: &'a Workspace,
    tools: &'a ToolRegistry,
    tool_hooks: &'a ToolHookSnapshot,
    event_tx: &'a mpsc::Sender<CoreEvent>,
    control_rx: &'a mut mpsc::Receiver<AgentLoopControl>,
    next_approval_id: &'a mut u64,
    cancellation: CancellationToken,
}

#[derive(Debug)]
pub(crate) enum TurnOutcome {
    Completed {
        messages: Vec<AgentMessage>,
        next_message_id: u64,
        next_approval_id: u64,
    },
    Cancelled {
        messages: Vec<AgentMessage>,
        next_message_id: u64,
        next_approval_id: u64,
    },
    Failed {
        error: String,
    },
}

pub(crate) async fn run_agent_loop(input: AgentLoopInput) -> TurnOutcome {
    match run_agent_loop_inner(input).await {
        Ok(outcome) => outcome,
        Err(error) => TurnOutcome::Failed {
            error: format!("{error:#}"),
        },
    }
}

async fn run_agent_loop_inner(input: AgentLoopInput) -> anyhow::Result<TurnOutcome> {
    let AgentLoopInput {
        turn_id,
        next_message_id,
        next_approval_id,
        mut session,
        model,
        tools,
        workspace,
        tool_hooks,
        event_tx,
        mut control_rx,
        steering,
        cancellation,
    } = input;

    let mut ids = MessageIdAllocator::new(next_message_id);
    let mut next_approval_id = next_approval_id;
    let mut produced_messages = Vec::new();

    loop {
        if cancellation.is_cancelled() {
            return Ok(TurnOutcome::Cancelled {
                messages: produced_messages,
                next_message_id: ids.next_raw(),
                next_approval_id,
            });
        }

        // Deliver ALL queued steering messages as context before the next model
        // response. Steering is drained only at these natural turn boundaries, never
        // mid-stream, so a steered message never aborts the in-flight reply. Like pi,
        // the whole pending queue is drained at once so the model sees every queued
        // message in a single request (one response to them all).
        let steers = drain_steers(&steering).await;
        inject_steered_messages(
            &event_tx,
            turn_id,
            &mut ids,
            &mut session,
            &mut produced_messages,
            steers,
        )
        .await?;

        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: Some(&tools),
        });
        let assistant_message_id = ids.next();
        let outcome = run_model_turn(ModelTurnInput {
            turn_id,
            assistant_message_id,
            request,
            model: Arc::clone(&model),
            event_tx: event_tx.clone(),
            cancellation: cancellation.child_token(),
        })
        .await?;

        match outcome {
            ModelTurnOutcome::Completed {
                assistant_message,
                tool_calls,
                stop_reason,
            } => {
                session.messages.push(assistant_message.clone());
                produced_messages.push(assistant_message);

                // The provider aborted the stream on its own; preserve any partial
                // output by treating it as a clean interrupt rather than a failure.
                if matches!(stop_reason, StopReason::Aborted) {
                    return Ok(TurnOutcome::Cancelled {
                        messages: produced_messages,
                        next_message_id: ids.next_raw(),
                        next_approval_id,
                    });
                }

                let has_tool_calls = !tool_calls.is_empty();
                match classify_stop_reason(stop_reason, has_tool_calls)? {
                    LoopAction::ExecuteTools => {}
                    LoopAction::Finish => {
                        // Steers may have arrived while this response was streaming. If
                        // any did, deliver them all and keep the agent running (one
                        // response to the whole batch) instead of stopping.
                        let steers = drain_steers(&steering).await;
                        if !steers.is_empty() {
                            inject_steered_messages(
                                &event_tx,
                                turn_id,
                                &mut ids,
                                &mut session,
                                &mut produced_messages,
                                steers,
                            )
                            .await?;
                            continue;
                        }
                        return complete_agent_loop(
                            &event_tx,
                            turn_id,
                            produced_messages,
                            ids.next_raw(),
                            next_approval_id,
                        )
                        .await;
                    }
                }

                if tool_calls.len() > MAX_TOOL_CALLS_PER_MODEL_TURN {
                    bail!(
                        "model requested {} tool calls, maximum is {}",
                        tool_calls.len(),
                        MAX_TOOL_CALLS_PER_MODEL_TURN
                    );
                }

                let tools_completed = execute_tool_requests(ExecuteToolRequestsInput {
                    tool_calls,
                    ids: &mut ids,
                    session: &mut session,
                    produced_messages: &mut produced_messages,
                    turn_id,
                    workspace: &workspace,
                    tools: &tools,
                    tool_hooks: &tool_hooks,
                    event_tx: &event_tx,
                    control_rx: &mut control_rx,
                    next_approval_id: &mut next_approval_id,
                    cancellation: cancellation.child_token(),
                })
                .await;
                if !tools_completed {
                    return Ok(TurnOutcome::Cancelled {
                        messages: produced_messages,
                        next_message_id: ids.next_raw(),
                        next_approval_id,
                    });
                }
            }
            ModelTurnOutcome::Interrupted { assistant_message } => {
                // Preserve the partial assistant reply in the session so a follow-up
                // turn (or the next user submission) continues from it.
                session.messages.push(assistant_message.clone());
                produced_messages.push(assistant_message);
                return Ok(TurnOutcome::Cancelled {
                    messages: produced_messages,
                    next_message_id: ids.next_raw(),
                    next_approval_id,
                });
            }
        }
    }
}

/// Drains ALL queued steered messages from the shared persistent queue, preserving
/// arrival order. Runs only at turn boundaries; delivery never aborts the in-flight
/// reply. The queue is owned by the runtime, so anything undelivered here survives
/// into a future turn instead of being dropped.
async fn drain_steers(steering: &Arc<Mutex<VecDeque<String>>>) -> Vec<String> {
    // Only held for the VecDeque drain — never across an await — so the critical
    // section is bounded to a few pointer swaps.
    steering.lock().await.drain(..).collect()
}

/// Emits and appends each steered user message to the session/produced messages so the
/// next model request carries the whole batch as context.
async fn inject_steered_messages(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    ids: &mut MessageIdAllocator,
    session: &mut SessionState,
    produced_messages: &mut Vec<AgentMessage>,
    texts: Vec<String>,
) -> anyhow::Result<()> {
    for text in texts {
        let message_id = ids.next();
        emit_user_message(event_tx, turn_id, message_id, &text).await?;
        let user_message = AgentMessage::User {
            id: message_id,
            content: vec![ContentBlock::Text { text: text.clone() }],
            timestamp: SystemTime::now(),
        };
        session.messages.push(user_message.clone());
        produced_messages.push(user_message);
    }
    Ok(())
}

async fn emit_user_message(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    message_id: MessageId,
    text: &str,
) -> anyhow::Result<()> {
    event_tx
        .send(CoreEvent::MessageStarted {
            turn_id: turn_id.0,
            message_id: message_id.0,
            role: MessageRole::User,
        })
        .await
        .context("failed to emit user message start")?;
    event_tx
        .send(CoreEvent::MessageDelta {
            turn_id: turn_id.0,
            message_id: message_id.0,
            delta: MessageDelta::Text(text.to_string()),
        })
        .await
        .context("failed to emit user message delta")?;
    event_tx
        .send(CoreEvent::MessageFinished {
            turn_id: turn_id.0,
            message_id: message_id.0,
        })
        .await
        .context("failed to emit user message finish")?;
    Ok(())
}

fn classify_stop_reason(
    stop_reason: StopReason,
    has_tool_calls: bool,
) -> anyhow::Result<LoopAction> {
    match stop_reason {
        StopReason::ToolUse if has_tool_calls => Ok(LoopAction::ExecuteTools),
        StopReason::ToolUse => bail!("provider returned ToolUse without tool calls"),
        StopReason::Stop | StopReason::Length if !has_tool_calls => Ok(LoopAction::Finish),
        StopReason::Stop | StopReason::Length => {
            bail!("provider returned {stop_reason:?} with tool calls")
        }
        StopReason::Error | StopReason::Aborted => {
            bail!("model stream ended with {stop_reason:?}")
        }
    }
}

async fn complete_agent_loop(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    messages: Vec<AgentMessage>,
    next_message_id: u64,
    next_approval_id: u64,
) -> anyhow::Result<TurnOutcome> {
    emit_turn_finished(event_tx, turn_id).await;
    Ok(TurnOutcome::Completed {
        messages,
        next_message_id,
        next_approval_id,
    })
}

async fn execute_tool_requests(input: ExecuteToolRequestsInput<'_>) -> bool {
    for tool_call in input.tool_calls {
        let tool_result = execute_one_tool_request(
            tool_call,
            input.ids.next(),
            input.session,
            input.turn_id,
            input.workspace,
            input.tools,
            input.tool_hooks,
            input.event_tx,
            input.control_rx,
            input.next_approval_id,
            input.cancellation.child_token(),
        )
        .await;
        let Ok(ToolExecutionOutcome::Completed(tool_result)) = tool_result else {
            return false;
        };
        input.session.messages.push(tool_result.clone());
        input.produced_messages.push(tool_result);
    }
    true
}

async fn execute_one_tool_request(
    tool_call: ToolCallRequest,
    message_id: MessageId,
    session: &SessionState,
    turn_id: TurnId,
    workspace: &Workspace,
    tools: &ToolRegistry,
    tool_hooks: &ToolHookSnapshot,
    event_tx: &mpsc::Sender<CoreEvent>,
    control_rx: &mut mpsc::Receiver<AgentLoopControl>,
    next_approval_id: &mut u64,
    cancellation: CancellationToken,
) -> Result<ToolExecutionOutcome> {
    match tool_call {
        ToolCallRequest::Ready(call) => {
            execute_tool_call(ToolExecutionInput {
                session_id: session.id,
                session,
                turn_id,
                message_id,
                call,
                cwd: PathBuf::from(&session.cwd),
                workspace: workspace.clone(),
                tools,
                tool_hooks: tool_hooks.clone(),
                event_tx: event_tx.clone(),
                control_rx,
                next_approval_id,
                cancellation,
            })
            .await
        }
        ToolCallRequest::Invalid(call) => Ok(ToolExecutionOutcome::Completed(
            invalid_tool_call_result(turn_id, message_id, call, event_tx.clone()).await,
        )),
    }
}

async fn emit_turn_finished(event_tx: &mpsc::Sender<CoreEvent>, turn_id: TurnId) {
    let _ = event_tx
        .send(CoreEvent::TurnFinished { turn_id: turn_id.0 })
        .await;
    let _ = event_tx.send(CoreEvent::AgentFinished).await;
}

struct MessageIdAllocator {
    next: u64,
}

impl MessageIdAllocator {
    fn new(next: u64) -> Self {
        Self { next }
    }

    fn next(&mut self) -> MessageId {
        let id = MessageId(self.next);
        self.next = self.next.saturating_add(1);
        id
    }

    fn next_raw(&self) -> u64 {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    use futures_util::stream::unfold;
    use iyon_api::{
        ModelApi, ModelRequest, ModelStream, ModelStreamEvent, ModelStreamFuture, StopReason,
    };
    use tokio_util::sync::CancellationToken;

    use super::{AgentLoopInput, TurnOutcome, run_agent_loop};
    use crate::CoreEvent;
    use crate::agent::control::AgentLoopControl;
    use crate::agent::transcript::AgentMessage;
    use crate::fs::{FsPermissions, Workspace};
    use crate::ids::{SessionId, TurnId};
    use crate::session::state::SessionState;
    use crate::tools::{ToolHookSet, ToolRegistry};

    /// Yields a pre-queued scripted stream on each `stream()` call, so a single agent
    /// loop can be driven through multiple model turns (e.g. steer → continuation).
    struct QueueModel {
        streams: Arc<std::sync::Mutex<VecDeque<tokio::sync::mpsc::Receiver<ModelStreamEvent>>>>,
    }

    impl ModelApi for QueueModel {
        fn stream(&self, _request: ModelRequest) -> ModelStreamFuture<'_> {
            let streams = Arc::clone(&self.streams);
            Box::pin(async move {
                let rx = streams
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("a scripted stream should be queued for each model turn");
                let stream: ModelStream = Box::pin(unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|event| (Ok(event), rx))
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
        let model = Arc::new(QueueModel {
            streams: Arc::new(std::sync::Mutex::new(streams)),
        });
        (model, senders)
    }

    fn loop_input(
        model: Arc<QueueModel>,
        event_tx: tokio::sync::mpsc::Sender<CoreEvent>,
        control_rx: tokio::sync::mpsc::Receiver<AgentLoopControl>,
        steering: Arc<Mutex<VecDeque<String>>>,
        cancellation: CancellationToken,
    ) -> AgentLoopInput {
        let mut session = SessionState::new(SessionId(1), PathBuf::from("/"));
        session.messages.push(AgentMessage::User {
            id: crate::ids::MessageId(100),
            content: vec![iyon_api::ContentBlock::Text {
                text: "initial prompt".to_string(),
            }],
            timestamp: std::time::SystemTime::now(),
        });
        AgentLoopInput {
            turn_id: TurnId(1),
            next_message_id: 101,
            next_approval_id: 1,
            session,
            model,
            tools: ToolRegistry::new(),
            workspace: Workspace::new(PathBuf::from("/"), FsPermissions::default()),
            tool_hooks: ToolHookSet::default().snapshot(),
            event_tx,
            control_rx,
            steering,
            cancellation,
        }
    }

    /// A fresh shared steering queue (test holds a clone to push into).
    fn steering() -> Arc<Mutex<VecDeque<String>>> {
        Arc::new(Mutex::new(VecDeque::new()))
    }

    /// Drains `event_rx` until an assistant text delta containing `needle` is seen.
    async fn wait_for_assistant_text(
        event_rx: &mut tokio::sync::mpsc::Receiver<CoreEvent>,
        needle: &str,
    ) {
        loop {
            let Some(event) = event_rx.recv().await else {
                panic!("event stream closed before {needle:?} observed");
            };
            if let CoreEvent::MessageDelta {
                delta: crate::MessageDelta::Text(text),
                ..
            } = event
            {
                if text.contains(needle) {
                    return;
                }
            }
        }
    }

    fn assistant_text(message: &AgentMessage, needle: &str) -> bool {
        let AgentMessage::Assistant { content, .. } = message else {
            return false;
        };
        content.iter().any(|block| match block {
            iyon_api::ContentBlock::Text { text } => text.contains(needle),
            _ => false,
        })
    }

    #[tokio::test]
    async fn cancel_returns_partials_for_commit() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        let (model, senders) = queued(1);
        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(8);
        let cancel = CancellationToken::new();

        let handle = tokio::spawn(run_agent_loop(loop_input(
            model,
            event_tx.clone(),
            control_rx,
            steering(),
            cancel.clone(),
        )));

        senders[0].send(ModelStreamEvent::Started).await.unwrap();
        senders[0]
            .send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "partial reply".to_string(),
            })
            .await
            .unwrap();
        wait_for_assistant_text(&mut event_rx, "partial").await;

        // Esc-interrupt == cancellation token; the partial reply is preserved.
        cancel.cancel();

        let outcome = handle.await.unwrap();
        let TurnOutcome::Cancelled { messages, .. } = outcome else {
            panic!("expected Cancelled, got {outcome:?}");
        };
        assert!(
            assistant_text(&messages[0], "partial"),
            "messages: {messages:?}"
        );
    }

    #[tokio::test]
    async fn steer_waits_for_response_to_complete_then_continues() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        // Two model turns: a full first response, then the answer to the steer.
        let (model, senders) = queued(2);
        let steering = steering();
        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(8);

        let handle = tokio::spawn(run_agent_loop(loop_input(
            model,
            event_tx.clone(),
            control_rx,
            steering.clone(),
            CancellationToken::new(),
        )));

        // Turn 1: a FULL response that reaches Stop. A steer is queued while it
        // streams, but must NOT interrupt it — the response runs to completion first.
        senders[0].send(ModelStreamEvent::Started).await.unwrap();
        senders[0]
            .send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "first full reply".to_string(),
            })
            .await
            .unwrap();
        wait_for_assistant_text(&mut event_rx, "first full reply").await;

        steering.lock().await.push_back("steer!".to_string());
        senders[0]
            .send(ModelStreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .await
            .unwrap();

        // Turn 2: answer the steered message and stop.
        senders[1].send(ModelStreamEvent::Started).await.unwrap();
        senders[1]
            .send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "the answer".to_string(),
            })
            .await
            .unwrap();
        senders[1]
            .send(ModelStreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .await
            .unwrap();

        let outcome = handle.await.unwrap();
        let TurnOutcome::Completed { messages, .. } = outcome else {
            panic!("expected Completed, got {outcome:?}");
        };

        // [full assistant (not interrupted), steered user, answer assistant]
        assert_eq!(messages.len(), 3, "messages: {messages:?}");
        assert!(
            assistant_text(&messages[0], "first full reply"),
            "messages: {messages:?}"
        );
        let AgentMessage::User { content, .. } = &messages[1] else {
            panic!("expected user message at index 1: {messages:?}")
        };
        assert!(
            content
                .iter()
                .any(|b| matches!(b, iyon_api::ContentBlock::Text { text } if text == "steer!")),
            "messages: {messages:?}"
        );
        assert!(
            assistant_text(&messages[2], "the answer"),
            "messages: {messages:?}"
        );
    }

    #[tokio::test]
    async fn multiple_steers_are_drained_at_once_into_one_response() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        // Two model turns: the full first response, then ONE answer to the whole batch.
        let (model, senders) = queued(2);
        let steering = steering();
        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(8);

        let handle = tokio::spawn(run_agent_loop(loop_input(
            model,
            event_tx.clone(),
            control_rx,
            steering.clone(),
            CancellationToken::new(),
        )));

        // Turn 1: a full response; three steers queue while it streams.
        senders[0].send(ModelStreamEvent::Started).await.unwrap();
        senders[0]
            .send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "first reply".to_string(),
            })
            .await
            .unwrap();
        wait_for_assistant_text(&mut event_rx, "first reply").await;

        for text in ["a", "b", "c"] {
            steering.lock().await.push_back(text.to_string());
        }
        senders[0]
            .send(ModelStreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .await
            .unwrap();

        // Turn 2: a single response addressing all three steers at once.
        senders[1].send(ModelStreamEvent::Started).await.unwrap();
        senders[1]
            .send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "batch answer".to_string(),
            })
            .await
            .unwrap();
        senders[1]
            .send(ModelStreamEvent::Done {
                stop_reason: StopReason::Stop,
            })
            .await
            .unwrap();

        let outcome = handle.await.unwrap();
        let TurnOutcome::Completed { messages, .. } = outcome else {
            panic!("expected Completed, got {outcome:?}");
        };

        // [assistant1, User(a), User(b), User(c), assistant2] — all steers are injected
        // together, feeding a single follow-up response.
        assert_eq!(messages.len(), 5, "messages: {messages:?}");
        assert!(
            assistant_text(&messages[0], "first reply"),
            "messages: {messages:?}"
        );
        for idx in 1..=3 {
            assert!(
                matches!(&messages[idx], AgentMessage::User { .. }),
                "message {idx} should be an injected steer: {messages:?}"
            );
        }
        assert!(
            assistant_text(&messages[4], "batch answer"),
            "messages: {messages:?}"
        );
    }
}
