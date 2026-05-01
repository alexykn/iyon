use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow, bail};
use iyon_api::{ModelApi, StopReason};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreEvent,
    agent::{
        control::AgentLoopControl,
        request::{RequestBuildContext, build_model_request},
        tool_call::ToolCallRequest,
        tool_execution::{
            ToolExecutionInput, ToolExecutionOutcome, execute_tool_call, invalid_tool_call_result,
        },
        transcript::AgentMessage,
        turn::{ModelTurnInput, run_model_turn},
    },
    fs::Workspace,
    ids::{MessageId, TurnId},
    session::state::SessionState,
    tools::ToolRegistry,
};

const MAX_AGENT_LOOP_ITERATIONS: usize = 8;
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
    pub event_tx: mpsc::Sender<CoreEvent>,
    pub control_rx: mpsc::Receiver<AgentLoopControl>,
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
    Failed {
        error: String,
    },
    Cancelled,
}

pub(crate) async fn run_agent_loop(input: AgentLoopInput) -> TurnOutcome {
    match run_agent_loop_inner(input).await {
        Ok(outcome) => outcome,
        Err(error) => TurnOutcome::Failed {
            error: error.to_string(),
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
        event_tx,
        mut control_rx,
        cancellation,
    } = input;

    let mut ids = MessageIdAllocator::new(next_message_id);
    let mut next_approval_id = next_approval_id;
    let mut produced_messages = Vec::new();

    for _ in 0..MAX_AGENT_LOOP_ITERATIONS {
        if cancellation.is_cancelled() {
            return Ok(TurnOutcome::Cancelled);
        }
        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: Some(&tools),
        });
        let assistant_message_id = ids.next();
        let turn = run_model_turn(ModelTurnInput {
            turn_id,
            assistant_message_id,
            request,
            model: Arc::clone(&model),
            event_tx: event_tx.clone(),
            cancellation: cancellation.child_token(),
        });
        let turn = tokio::select! {
            _ = cancellation.cancelled() => return Ok(TurnOutcome::Cancelled),
            turn = turn => turn?,
        };

        let tool_calls = turn.tool_calls;
        let has_tool_calls = !tool_calls.is_empty();
        let stop_reason = turn.stop_reason;
        let assistant_message = turn.assistant_message;

        session.messages.push(assistant_message.clone());
        produced_messages.push(assistant_message);

        match classify_stop_reason(stop_reason, has_tool_calls)? {
            LoopAction::ExecuteTools => {}
            LoopAction::Finish => {
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
            event_tx: &event_tx,
            control_rx: &mut control_rx,
            next_approval_id: &mut next_approval_id,
            cancellation: cancellation.child_token(),
        })
        .await;
        if !tools_completed {
            return Ok(TurnOutcome::Cancelled);
        }
    }

    Err(anyhow!(
        "agent loop exceeded maximum iterations ({MAX_AGENT_LOOP_ITERATIONS})"
    ))
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
    event_tx: &mpsc::Sender<CoreEvent>,
    control_rx: &mut mpsc::Receiver<AgentLoopControl>,
    next_approval_id: &mut u64,
    cancellation: CancellationToken,
) -> Result<ToolExecutionOutcome> {
    match tool_call {
        ToolCallRequest::Ready(call) => {
            execute_tool_call(ToolExecutionInput {
                session_id: session.id,
                turn_id,
                message_id,
                call,
                cwd: PathBuf::from(&session.cwd),
                workspace: workspace.clone(),
                tools,
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
