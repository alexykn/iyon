use std::{path::PathBuf, time::SystemTime};

use anyhow::Result;
use iyon_api::ContentBlock;
use serde_json::{Value, json};
use thiserror::Error;
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
    tools::{
        AfterToolCallContext, BeforeToolCallContext, BeforeToolCallResolution, ToolApprovalPolicy,
        ToolContext, ToolHookSnapshot, ToolRegistry, ToolResult, ToolUpdateSink,
    },
};

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

pub(crate) struct ToolExecutionInput<'a> {
    pub session_id: SessionId,
    pub session: &'a crate::session::state::SessionState,
    pub turn_id: TurnId,
    pub message_id: MessageId,
    pub call: AssembledToolCall,
    pub cwd: PathBuf,
    pub workspace: Workspace,
    pub tools: &'a ToolRegistry,
    pub tool_hooks: ToolHookSnapshot,
    pub event_tx: mpsc::Sender<CoreEvent>,
    pub control_rx: &'a mut mpsc::Receiver<AgentLoopControl>,
    pub next_approval_id: &'a mut u64,
    pub cancellation: CancellationToken,
}

pub(crate) enum ToolExecutionOutcome {
    Completed(AgentMessage),
    Cancelled,
}

#[derive(Debug, Error)]
enum ApprovalInputError {
    #[error("tool not found")]
    ToolNotFound,
}

impl<'a, 'b> TryFrom<&'a mut ToolExecutionInput<'b>> for ApprovalInput<'a>
where
    'b: 'a,
{
    type Error = ApprovalInputError;

    fn try_from(input: &'a mut ToolExecutionInput<'b>) -> Result<Self, Self::Error> {
        let definition = input
            .tools
            .get(&input.call.name)
            .ok_or(ApprovalInputError::ToolNotFound)
            .map(|t| t.definition())?;

        Ok(ApprovalInput {
            event_tx: &input.event_tx,
            control_rx: input.control_rx, // mutable borrow
            next_approval_id: input.next_approval_id,
            turn_id: input.turn_id,
            message_id: input.message_id,
            call: &input.call,
            policy: effective_approval_policy(definition.approval, &input.call),
            cancellation: input.cancellation.clone(),
        })
    }
}

fn effective_approval_policy(
    policy: ToolApprovalPolicy,
    call: &AssembledToolCall,
) -> ToolApprovalPolicy {
    if call.name == "bash" && bash_command_uses_sudo(&call.arguments) {
        return ToolApprovalPolicy::AlwaysAsk;
    }
    policy
}

fn bash_command_uses_sudo(arguments: &Value) -> bool {
    let Some(command) = arguments.get("command").and_then(Value::as_str) else {
        return false;
    };

    command
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(' | ')'))
        .any(|token| token == "sudo")
}

impl<'a> From<&'a ToolExecutionInput<'a>> for ToolContext {
    fn from(input: &'a ToolExecutionInput<'a>) -> Self {
        ToolContext {
            session_id: input.session_id,
            turn_id: input.turn_id,
            tool_call_id: input.call.id.clone(), // assuming this exists
            cwd: input.cwd.clone(),
            workspace: input.workspace.clone(),
            cancellation: input.cancellation.clone(),
        }
    }
}

pub(crate) async fn execute_tool_call(
    mut input: ToolExecutionInput<'_>,
) -> Result<ToolExecutionOutcome> {
    emit_tool_started(
        &input.event_tx,
        input.turn_id,
        input.message_id,
        &input.call,
    )
    .await;
    let message = match input.tools.get(&input.call.name) {
        Some(tool) => {
            let prepared_args = match run_before_hooks(&input).await {
                Ok(BeforeToolCallResolution::Proceed { args }) => args,
                Ok(BeforeToolCallResolution::Block { reason }) => {
                    let text = reason.unwrap_or_else(|| "Tool execution was blocked".to_string());
                    let message = error_tool_result_message(
                        input.message_id,
                        input.call.id.clone(),
                        input.call.name.clone(),
                        text,
                    );
                    emit_tool_result_message(&input.event_tx, input.turn_id, &message).await;
                    emit_tool_finished(
                        &input.event_tx,
                        input.turn_id,
                        input.message_id,
                        &input.call,
                        true,
                    )
                    .await;
                    return Ok(ToolExecutionOutcome::Completed(message));
                }
                Err(error) => {
                    let message = error_tool_result_message(
                        input.message_id,
                        input.call.id.clone(),
                        input.call.name.clone(),
                        format!("Before-tool hook failed: {error}"),
                    );
                    emit_tool_result_message(&input.event_tx, input.turn_id, &message).await;
                    emit_tool_finished(
                        &input.event_tx,
                        input.turn_id,
                        input.message_id,
                        &input.call,
                        true,
                    )
                    .await;
                    return Ok(ToolExecutionOutcome::Completed(message));
                }
            };

            let decision = await_approval_if_required(ApprovalInput::try_from(&mut input)?).await;

            match decision {
                ApprovalDecision::Approved => {
                    let ctx = ToolContext::from(&input);
                    let updates = ToolUpdateSink::new(input.event_tx.clone(), &input);
                    let mut result = match tool.execute(ctx, prepared_args.clone(), updates).await {
                        Ok(result) => result,
                        Err(error) => ToolResult {
                            content: vec![ContentBlock::Text {
                                text: format!("Tool \"{}\" failed: {error}", input.call.name),
                            }],
                            details: json!({}),
                            is_error: true,
                            terminate: false,
                        },
                    };
                    match run_after_hooks(&input, &prepared_args, &result).await {
                        Ok(patch) => apply_after_patch(&mut result, patch),
                        Err(error) => {
                            result = ToolResult {
                                content: vec![ContentBlock::Text {
                                    text: format!("After-tool hook failed: {error}"),
                                }],
                                details: json!({}),
                                is_error: true,
                                terminate: false,
                            }
                        }
                    }

                    tool_result_message(
                        input.message_id,
                        input.call.id.clone(),
                        input.call.name.clone(),
                        result.content,
                        result.details,
                        result.is_error,
                    )
                }
                ApprovalDecision::Rejected { reason } => error_tool_result_message(
                    input.message_id,
                    input.call.id.clone(),
                    input.call.name.clone(),
                    format!(
                        "Tool execution rejected: {}",
                        reason.unwrap_or_else(|| "approval denied".to_string())
                    ),
                ),
                ApprovalDecision::Cancelled => return Ok(ToolExecutionOutcome::Cancelled),
            }
        }
        None => error_tool_result_message(
            input.message_id,
            input.call.id.clone(),
            input.call.name.clone(),
            format!("Tool \"{}\" is not available.", input.call.name),
        ),
    };

    emit_tool_result_message(&input.event_tx, input.turn_id, &message).await;
    emit_tool_finished(
        &input.event_tx,
        input.turn_id,
        input.message_id,
        &input.call,
        tool_result_is_error(&message),
    )
    .await;

    Ok(ToolExecutionOutcome::Completed(message))
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
            () = cancellation.cancelled() => return ApprovalDecision::Cancelled,
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

async fn run_before_hooks(input: &ToolExecutionInput<'_>) -> Result<BeforeToolCallResolution> {
    input
        .tool_hooks
        .run_before_hooks(BeforeToolCallContext {
            turn_id: input.turn_id,
            tool_call_id: &input.call.id,
            tool_name: &input.call.name,
            args: &input.call.arguments,
            session: input.session,
        })
        .await
}

async fn run_after_hooks(
    input: &ToolExecutionInput<'_>,
    args: &Value,
    result: &ToolResult,
) -> Result<crate::tools::AfterToolCallPatch> {
    input
        .tool_hooks
        .run_after_hooks(AfterToolCallContext {
            turn_id: input.turn_id,
            tool_call_id: &input.call.id,
            tool_name: &input.call.name,
            args,
            result,
            session: input.session,
        })
        .await
}

fn apply_after_patch(result: &mut ToolResult, patch: crate::tools::AfterToolCallPatch) {
    if let Some(content) = patch.content_override {
        result.content = content;
    }
    if let Some(details) = patch.details_override {
        result.details = details;
    }
    if let Some(is_error) = patch.is_error_override {
        result.is_error = is_error;
    }
    if let Some(terminate) = patch.terminate_override {
        result.terminate = terminate;
    }
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
            arguments: call.arguments.clone(),
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
        details,
        is_error: _,
        ..
    } = message
    else {
        return;
    };

    let AgentMessage::ToolResult {
        tool_call_id,
        tool_name,
        is_error,
        ..
    } = message
    else {
        return;
    };

    let _ = event_tx
        .send(CoreEvent::ToolResultStarted {
            turn_id: turn_id.0,
            message_id: id.0,
            tool_call_id: tool_call_id.0.clone(),
            tool_name: tool_name.clone(),
            is_error: *is_error,
        })
        .await;
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
    let _ = event_tx
        .send(CoreEvent::ToolResultFinished {
            turn_id: turn_id.0,
            message_id: id.0,
            tool_call_id: tool_call_id.0.clone(),
            tool_name: tool_name.clone(),
            text: text_content(content),
            details: details.clone(),
            is_error: *is_error,
        })
        .await;
}

fn text_content(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_result_is_error(message: &AgentMessage) -> bool {
    matches!(message, AgentMessage::ToolResult { is_error: true, .. })
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };

    use serde_json::json;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::{
        agent::{tool_call::AssembledToolCall, transcript::AgentMessage},
        fs::{FsPermissions, Workspace},
        ids::{MessageId, SessionId, ToolCallId, TurnId},
        session::state::SessionState,
        tools::{
            AfterToolCallContext, AfterToolCallHook, AfterToolCallPatch, BeforeToolCallContext,
            BeforeToolCallDecision, BeforeToolCallHook, ToolApprovalPolicy, ToolContext,
            ToolDefinition, ToolExecutionMode, ToolExecutor, ToolFuture, ToolHookSet, ToolResult,
            ToolSource, ToolUpdateSink,
        },
    };

    use super::{ToolExecutionInput, ToolExecutionOutcome, execute_tool_call};

    struct TestTool {
        called: Arc<AtomicBool>,
        seen_input: Arc<Mutex<Option<serde_json::Value>>>,
        result: ToolResult,
    }

    impl ToolExecutor for TestTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "test".to_string(),
                label: "test".to_string(),
                description: "test tool".to_string(),
                input_schema: json!({"type":"object"}),
                execution_mode: ToolExecutionMode::Sequential,
                approval: ToolApprovalPolicy::NeverAsk,
                source: ToolSource::Builtin,
                prompt_snippet: None,
                prompt_guidelines: vec![],
            }
        }

        fn execute(
            &self,
            _ctx: ToolContext,
            input: serde_json::Value,
            _updates: ToolUpdateSink,
        ) -> ToolFuture<'_> {
            self.called.store(true, Ordering::SeqCst);
            *self.seen_input.lock().unwrap() = Some(input);
            let result = self.result.clone();
            Box::pin(async move { Ok(result) })
        }
    }

    struct BlockBeforeHook;
    impl BeforeToolCallHook for BlockBeforeHook {
        fn before_tool_call<'a>(
            &'a self,
            _ctx: BeforeToolCallContext<'a>,
        ) -> crate::tools::hooks::BeforeHookFuture<'a> {
            Box::pin(async {
                Ok(BeforeToolCallDecision::Block {
                    reason: Some("blocked".to_string()),
                })
            })
        }
    }

    struct PatchArgsHookOne;
    impl BeforeToolCallHook for PatchArgsHookOne {
        fn before_tool_call<'a>(
            &'a self,
            _ctx: BeforeToolCallContext<'a>,
        ) -> crate::tools::hooks::BeforeHookFuture<'a> {
            Box::pin(async { Ok(BeforeToolCallDecision::PatchArgs(json!({"one": true}))) })
        }
    }

    struct PatchArgsHookTwo;
    impl BeforeToolCallHook for PatchArgsHookTwo {
        fn before_tool_call<'a>(
            &'a self,
            ctx: BeforeToolCallContext<'a>,
        ) -> crate::tools::hooks::BeforeHookFuture<'a> {
            let mut next = ctx.args.clone();
            next["two"] = json!(true);
            Box::pin(async move { Ok(BeforeToolCallDecision::PatchArgs(next)) })
        }
    }

    struct PatchAfterHook;
    impl AfterToolCallHook for PatchAfterHook {
        fn after_tool_call<'a>(
            &'a self,
            _ctx: AfterToolCallContext<'a>,
        ) -> crate::tools::hooks::AfterHookFuture<'a> {
            Box::pin(async {
                Ok(Some(AfterToolCallPatch {
                    content_override: Some(vec![iyon_api::ContentBlock::Text {
                        text: "patched".to_string(),
                    }]),
                    details_override: Some(json!({"patched": true})),
                    is_error_override: Some(true),
                    terminate_override: None,
                }))
            })
        }
    }

    #[tokio::test]
    async fn before_hook_can_block_tool_execution() {
        let called = Arc::new(AtomicBool::new(false));
        let tool = Arc::new(TestTool {
            called: called.clone(),
            seen_input: Arc::new(Mutex::new(None)),
            result: ToolResult {
                content: vec![iyon_api::ContentBlock::Text {
                    text: "ok".to_string(),
                }],
                details: json!({}),
                is_error: false,
                terminate: false,
            },
        });

        let mut registry = crate::tools::ToolRegistry::new();
        registry.register(tool).unwrap();

        let mut hooks = ToolHookSet::default();
        hooks.register_before(Arc::new(BlockBeforeHook));

        let session = SessionState::new(SessionId(1), PathBuf::from("."));
        let workspace = Workspace::new(PathBuf::from("."), FsPermissions::default());
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (_control_tx, mut control_rx) = mpsc::channel(8);
        let mut next_approval_id = 1;

        let outcome = execute_tool_call(ToolExecutionInput {
            session_id: session.id,
            session: &session,
            turn_id: TurnId(1),
            message_id: MessageId(1),
            call: AssembledToolCall {
                id: ToolCallId("call-1".to_string()),
                name: "test".to_string(),
                arguments: json!({}),
            },
            cwd: PathBuf::from("."),
            workspace,
            tools: &registry,
            tool_hooks: hooks.snapshot(),
            event_tx,
            control_rx: &mut control_rx,
            next_approval_id: &mut next_approval_id,
            cancellation: CancellationToken::new(),
        })
        .await
        .unwrap();

        assert!(!called.load(Ordering::SeqCst));
        match outcome {
            ToolExecutionOutcome::Completed(AgentMessage::ToolResult {
                is_error, content, ..
            }) => {
                assert!(is_error);
                let text = content
                    .into_iter()
                    .find_map(|b| match b {
                        iyon_api::ContentBlock::Text { text } => Some(text),
                        _ => None,
                    })
                    .unwrap();
                assert!(text.contains("blocked"));
            }
            _ => panic!("expected completed tool result"),
        }
    }

    #[tokio::test]
    async fn after_hook_can_patch_tool_result() {
        let called = Arc::new(AtomicBool::new(false));
        let seen_input = Arc::new(Mutex::new(None));
        let tool = Arc::new(TestTool {
            called,
            seen_input: seen_input.clone(),
            result: ToolResult {
                content: vec![iyon_api::ContentBlock::Text {
                    text: "ok".to_string(),
                }],
                details: json!({"orig": true}),
                is_error: false,
                terminate: false,
            },
        });

        let mut registry = crate::tools::ToolRegistry::new();
        registry.register(tool).unwrap();

        let mut hooks = ToolHookSet::default();
        hooks.register_after(Arc::new(PatchAfterHook));

        let session = SessionState::new(SessionId(1), PathBuf::from("."));
        let workspace = Workspace::new(PathBuf::from("."), FsPermissions::default());
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (_control_tx, mut control_rx) = mpsc::channel(8);
        let mut next_approval_id = 1;

        let outcome = execute_tool_call(ToolExecutionInput {
            session_id: session.id,
            session: &session,
            turn_id: TurnId(1),
            message_id: MessageId(1),
            call: AssembledToolCall {
                id: ToolCallId("call-1".to_string()),
                name: "test".to_string(),
                arguments: json!({}),
            },
            cwd: PathBuf::from("."),
            workspace,
            tools: &registry,
            tool_hooks: hooks.snapshot(),
            event_tx,
            control_rx: &mut control_rx,
            next_approval_id: &mut next_approval_id,
            cancellation: CancellationToken::new(),
        })
        .await
        .unwrap();

        match outcome {
            ToolExecutionOutcome::Completed(AgentMessage::ToolResult {
                is_error,
                content,
                details,
                ..
            }) => {
                assert!(is_error);
                assert_eq!(details, json!({"patched": true}));
                let text = content
                    .into_iter()
                    .find_map(|b| match b {
                        iyon_api::ContentBlock::Text { text } => Some(text),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(text, "patched");
                assert_eq!(seen_input.lock().unwrap().clone(), Some(json!({})));
            }
            _ => panic!("expected completed tool result"),
        }
    }

    #[tokio::test]
    async fn before_hooks_patch_args_in_deterministic_chain_order() {
        let called = Arc::new(AtomicBool::new(false));
        let seen_input = Arc::new(Mutex::new(None));
        let tool = Arc::new(TestTool {
            called,
            seen_input: seen_input.clone(),
            result: ToolResult {
                content: vec![iyon_api::ContentBlock::Text {
                    text: "ok".to_string(),
                }],
                details: json!({}),
                is_error: false,
                terminate: false,
            },
        });

        let mut registry = crate::tools::ToolRegistry::new();
        registry.register(tool).unwrap();

        let mut hooks = ToolHookSet::default();
        hooks.register_before_fn(PatchArgsHookOne);
        hooks.register_before_fn(PatchArgsHookTwo);

        let session = SessionState::new(SessionId(1), PathBuf::from("."));
        let workspace = Workspace::new(PathBuf::from("."), FsPermissions::default());
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (_control_tx, mut control_rx) = mpsc::channel(8);
        let mut next_approval_id = 1;

        let outcome = execute_tool_call(ToolExecutionInput {
            session_id: session.id,
            session: &session,
            turn_id: TurnId(1),
            message_id: MessageId(1),
            call: AssembledToolCall {
                id: ToolCallId("call-1".to_string()),
                name: "test".to_string(),
                arguments: json!({"orig": true}),
            },
            cwd: PathBuf::from("."),
            workspace,
            tools: &registry,
            tool_hooks: hooks.snapshot(),
            event_tx,
            control_rx: &mut control_rx,
            next_approval_id: &mut next_approval_id,
            cancellation: CancellationToken::new(),
        })
        .await
        .unwrap();

        assert!(matches!(
            outcome,
            ToolExecutionOutcome::Completed(AgentMessage::ToolResult { .. })
        ));
        assert_eq!(
            seen_input.lock().unwrap().clone(),
            Some(json!({"one": true, "two": true}))
        );
    }
}
