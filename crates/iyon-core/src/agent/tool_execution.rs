use std::{path::PathBuf, time::SystemTime};

use iyon_api::ContentBlock;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreEvent, MessageDelta, MessageRole,
    agent::{
        control::AgentLoopControl,
        tool_call::{AssembledToolCall, InvalidToolCall},
        transcript::AgentMessage,
    },
    fs::Workspace,
    ids::{ApprovalId, MessageId, SessionId, TurnId},
    tools::{ToolApprovalPolicy, ToolContext, ToolRegistry, ToolUpdateSink},
};

pub(crate) struct ToolExecutionInput<'a> {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub message_id: MessageId,
    pub call: AssembledToolCall,
    pub cwd: PathBuf,
    pub workspace: Workspace,
    pub tools: &'a ToolRegistry,
    pub event_tx: mpsc::Sender<CoreEvent>,
    pub control_rx: &'a mut mpsc::Receiver<AgentLoopControl>,
    pub next_approval_id: &'a mut u64,
    pub cancellation: CancellationToken,
}

pub(crate) enum ToolExecutionOutcome {
    Completed(AgentMessage),
    Cancelled,
}

pub(crate) async fn execute_tool_call(input: ToolExecutionInput<'_>) -> ToolExecutionOutcome {
    let ToolExecutionInput {
        session_id,
        turn_id,
        message_id,
        call,
        cwd,
        workspace,
        tools,
        event_tx,
        control_rx,
        next_approval_id,
        cancellation,
    } = input;

    emit_tool_started(&event_tx, turn_id, message_id, &call).await;

    let message = match tools.get(&call.name) {
        Some(tool) => {
            let definition = tool.definition();
            match await_approval_if_required(ApprovalInput {
                event_tx: &event_tx,
                control_rx,
                next_approval_id,
                turn_id,
                message_id,
                call: &call,
                policy: definition.approval,
                cancellation: cancellation.clone(),
            })
            .await
            {
                ApprovalDecision::Approved => {
                    let ctx = ToolContext {
                        session_id,
                        turn_id,
                        tool_call_id: call.id.clone(),
                        cwd,
                        workspace,
                        cancellation: cancellation.clone(),
                    };
                    let updates = ToolUpdateSink::new(
                        event_tx.clone(),
                        turn_id,
                        message_id,
                        call.id.clone(),
                        call.name.clone(),
                        cancellation.clone(),
                    );
                    match tool.execute(ctx, call.arguments.clone(), updates).await {
                        Ok(result) => tool_result_message(
                            message_id,
                            call.id.clone(),
                            call.name.clone(),
                            result.content,
                            result.details,
                            result.is_error,
                        ),
                        Err(error) => error_tool_result_message(
                            message_id,
                            call.id.clone(),
                            call.name.clone(),
                            format!("Tool \"{}\" failed: {error}", call.name),
                        ),
                    }
                }
                ApprovalDecision::Rejected { reason } => error_tool_result_message(
                    message_id,
                    call.id.clone(),
                    call.name.clone(),
                    format!(
                        "Tool execution rejected: {}",
                        reason.unwrap_or_else(|| "approval denied".to_string())
                    ),
                ),
                ApprovalDecision::Cancelled => return ToolExecutionOutcome::Cancelled,
            }
        }
        None => error_tool_result_message(
            message_id,
            call.id.clone(),
            call.name.clone(),
            format!("Tool \"{}\" is not available.", call.name),
        ),
    };

    emit_tool_result_message(&event_tx, turn_id, &message).await;
    emit_tool_finished(
        &event_tx,
        turn_id,
        message_id,
        &call,
        tool_result_is_error(&message),
    )
    .await;

    ToolExecutionOutcome::Completed(message)
}

pub(crate) async fn invalid_tool_call_result(
    turn_id: TurnId,
    message_id: MessageId,
    call: InvalidToolCall,
    event_tx: mpsc::Sender<CoreEvent>,
) -> AgentMessage {
    let message = error_tool_result_message(
        message_id,
        call.id.clone(),
        call.name,
        format!("Invalid tool arguments: {}", call.error),
    );
    emit_tool_result_message(&event_tx, turn_id, &message).await;
    message
}

struct ApprovalInput<'a> {
    event_tx: &'a mpsc::Sender<CoreEvent>,
    control_rx: &'a mut mpsc::Receiver<AgentLoopControl>,
    next_approval_id: &'a mut u64,
    turn_id: TurnId,
    message_id: MessageId,
    call: &'a AssembledToolCall,
    policy: ToolApprovalPolicy,
    cancellation: CancellationToken,
}

enum ApprovalDecision {
    Approved,
    Rejected { reason: Option<String> },
    Cancelled,
}

async fn await_approval_if_required(input: ApprovalInput<'_>) -> ApprovalDecision {
    let ApprovalInput {
        event_tx,
        control_rx,
        next_approval_id,
        turn_id,
        message_id,
        call,
        policy,
        cancellation,
    } = input;

    if matches!(policy, ToolApprovalPolicy::NeverAsk) {
        return ApprovalDecision::Approved;
    }

    let approval_id = ApprovalId(*next_approval_id);
    *next_approval_id = (*next_approval_id).saturating_add(1);
    emit_approval_requested(event_tx, turn_id, message_id, approval_id, call).await;

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return ApprovalDecision::Cancelled,
            control = control_rx.recv() => match control {
                Some(AgentLoopControl::ApproveToolCall { approval_id: id }) if id == approval_id => {
                    emit_approval_resolved(event_tx, turn_id, approval_id, call, true, None).await;
                    return ApprovalDecision::Approved;
                }
                Some(AgentLoopControl::RejectToolCall { approval_id: id, reason }) if id == approval_id => {
                    emit_approval_resolved(event_tx, turn_id, approval_id, call, false, reason.clone()).await;
                    return ApprovalDecision::Rejected { reason };
                }
                Some(AgentLoopControl::Cancel) | None => return ApprovalDecision::Cancelled,
                Some(_) => {}
            }
        }
    }
}

async fn emit_approval_requested(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    message_id: MessageId,
    approval_id: ApprovalId,
    call: &AssembledToolCall,
) {
    let _ = event_tx
        .send(CoreEvent::ToolApprovalRequested {
            turn_id: turn_id.0,
            approval_id: approval_id.0,
            message_id: message_id.0,
            tool_call_id: call.id.0.clone(),
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        })
        .await;
}

async fn emit_approval_resolved(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    approval_id: ApprovalId,
    call: &AssembledToolCall,
    approved: bool,
    reason: Option<String>,
) {
    let _ = event_tx
        .send(CoreEvent::ToolApprovalResolved {
            turn_id: turn_id.0,
            approval_id: approval_id.0,
            tool_call_id: call.id.0.clone(),
            approved,
            reason,
        })
        .await;
}

fn tool_result_message(
    id: MessageId,
    tool_call_id: crate::ids::ToolCallId,
    tool_name: String,
    content: Vec<ContentBlock>,
    details: Value,
    is_error: bool,
) -> AgentMessage {
    AgentMessage::ToolResult {
        id,
        tool_call_id,
        tool_name,
        content,
        details,
        is_error,
        timestamp: SystemTime::now(),
    }
}

fn error_tool_result_message(
    id: MessageId,
    tool_call_id: crate::ids::ToolCallId,
    tool_name: String,
    text: String,
) -> AgentMessage {
    tool_result_message(
        id,
        tool_call_id,
        tool_name,
        vec![ContentBlock::Text { text }],
        json!({}),
        true,
    )
}

async fn emit_tool_started(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    message_id: MessageId,
    call: &AssembledToolCall,
) {
    let _ = event_tx
        .send(CoreEvent::ToolCallStarted {
            turn_id: turn_id.0,
            message_id: message_id.0,
            tool_call_id: call.id.0.clone(),
            tool_name: call.name.clone(),
        })
        .await;
}

async fn emit_tool_finished(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    message_id: MessageId,
    call: &AssembledToolCall,
    is_error: bool,
) {
    let _ = event_tx
        .send(CoreEvent::ToolCallFinished {
            turn_id: turn_id.0,
            message_id: message_id.0,
            tool_call_id: call.id.0.clone(),
            tool_name: call.name.clone(),
            is_error,
        })
        .await;
}

async fn emit_tool_result_message(
    event_tx: &mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    message: &AgentMessage,
) {
    let AgentMessage::ToolResult {
        id,
        content,
        is_error: _,
        ..
    } = message
    else {
        return;
    };

    let _ = event_tx
        .send(CoreEvent::MessageStarted {
            turn_id: turn_id.0,
            message_id: id.0,
            role: MessageRole::ToolResult,
        })
        .await;
    for block in content {
        if let ContentBlock::Text { text } = block {
            let _ = event_tx
                .send(CoreEvent::MessageDelta {
                    turn_id: turn_id.0,
                    message_id: id.0,
                    delta: MessageDelta::Text(text.clone()),
                })
                .await;
        }
    }
    let _ = event_tx
        .send(CoreEvent::MessageFinished {
            turn_id: turn_id.0,
            message_id: id.0,
        })
        .await;
}

fn tool_result_is_error(message: &AgentMessage) -> bool {
    matches!(message, AgentMessage::ToolResult { is_error: true, .. })
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, time::SystemTime};

    use serde_json::json;

    use super::*;
    use crate::{fs::FsPermissions, tools::builtin::read::ReadTool};

    #[tokio::test]
    async fn missing_tool_returns_error_result() {
        let registry = ToolRegistry::new();
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (_control_tx, mut control_rx) = mpsc::channel(8);
        let mut next_approval_id = 1;

        let message = execute_tool_call(ToolExecutionInput {
            session_id: SessionId(1),
            turn_id: TurnId(1),
            message_id: MessageId(1),
            call: AssembledToolCall {
                id: crate::ids::ToolCallId("call-1".to_string()),
                name: "missing".to_string(),
                arguments: json!({}),
            },
            cwd: PathBuf::from("/tmp"),
            workspace: Workspace::new(PathBuf::from("/tmp"), FsPermissions::default()),
            tools: &registry,
            event_tx,
            control_rx: &mut control_rx,
            next_approval_id: &mut next_approval_id,
            cancellation: CancellationToken::new(),
        })
        .await;

        assert!(matches!(
            message,
            ToolExecutionOutcome::Completed(AgentMessage::ToolResult { is_error: true, .. })
        ));
    }

    #[tokio::test]
    async fn read_tool_returns_file_content() {
        let root = create_temp_dir("tool-execution-read");
        fs::write(root.join("file.txt"), "hello").unwrap();
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(ReadTool)).unwrap();
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (_control_tx, mut control_rx) = mpsc::channel(8);
        let mut next_approval_id = 1;

        let message = execute_tool_call(ToolExecutionInput {
            session_id: SessionId(1),
            turn_id: TurnId(1),
            message_id: MessageId(1),
            call: AssembledToolCall {
                id: crate::ids::ToolCallId("call-1".to_string()),
                name: "read".to_string(),
                arguments: json!({ "path": "file.txt" }),
            },
            cwd: root.clone(),
            workspace: Workspace::new(root.clone(), FsPermissions::default()),
            tools: &registry,
            event_tx,
            control_rx: &mut control_rx,
            next_approval_id: &mut next_approval_id,
            cancellation: CancellationToken::new(),
        })
        .await;

        assert!(matches!(
            message,
            ToolExecutionOutcome::Completed(AgentMessage::ToolResult { content, is_error: false, .. })
                if matches!(&content[..], [ContentBlock::Text { text }] if text == "hello")
        ));
        let _ = fs::remove_dir_all(root);
    }

    fn create_temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "iyon-{prefix}-{}",
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
