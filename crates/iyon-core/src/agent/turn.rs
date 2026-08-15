use std::{sync::Arc, time::SystemTime};

use anyhow::{Context, bail};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreEvent, MessageDelta, MessageRole, ToolCallDelta,
    agent::{
        tool_call::{ToolCallAssembler, ToolCallRequest},
        transcript::AgentMessage,
    },
    ids::{MessageId, TurnId},
};
use iyon_api::{ContentBlock, ModelApi, ModelRequest, ModelStreamEvent, StopReason, Usage};

pub(crate) struct ModelTurnInput {
    pub turn_id: TurnId,
    pub assistant_message_id: MessageId,
    pub request: ModelRequest,
    pub model: Arc<dyn ModelApi>,
    pub event_tx: mpsc::Sender<CoreEvent>,
    pub cancellation: CancellationToken,
}

#[derive(Debug)]
pub(crate) enum ModelTurnOutcome {
    Completed {
        assistant_message: AgentMessage,
        tool_calls: Vec<ToolCallRequest>,
        stop_reason: StopReason,
    },
    Interrupted {
        assistant_message: AgentMessage,
    },
}

pub(crate) async fn run_model_turn(input: ModelTurnInput) -> anyhow::Result<ModelTurnOutcome> {
    let ModelTurnInput {
        turn_id,
        assistant_message_id,
        request,
        model,
        event_tx,
        cancellation,
    } = input;

    let mut stream = model
        .stream(request)
        .await
        .context("failed to start model stream")?;

    let mut content = Vec::new();
    let mut text = String::new();
    let mut thinking = String::new();
    let mut usage: Option<Usage> = None;
    let mut tool_calls = ToolCallAssembler::default();

    if !send_event(
        &event_tx,
        &cancellation,
        CoreEvent::MessageStarted {
            turn_id: turn_id.0,
            message_id: assistant_message_id.0,
            role: MessageRole::Assistant,
        },
    )
    .await?
    {
        return finish_interrupted(FinishModelTurnInput {
            event_tx: &event_tx,
            turn_id,
            assistant_message_id,
            content,
            text,
            thinking,
            usage,
            tool_calls,
            stop_reason: StopReason::Aborted,
            cancellation: cancellation.clone(),
        })
        .await;
    }

    loop {
        let event = tokio::select! {
            () = cancellation.cancelled() => {
                return finish_interrupted(FinishModelTurnInput {
                    event_tx: &event_tx,
                    turn_id,
                    assistant_message_id,
                    content,
                    text,
                    thinking,
                    usage,
                    tool_calls,
                    stop_reason: StopReason::Aborted,
            cancellation: cancellation.clone(),
                }).await;
            }
            event = stream.next() => event,
        };
        let Some(event) = event else {
            break;
        };
        match event.context("model stream error")? {
            ModelStreamEvent::Started | ModelStreamEvent::TextStart { .. } => {}
            ModelStreamEvent::TextDelta { delta, .. } => {
                if !handle_text_delta(
                    &event_tx,
                    &cancellation,
                    turn_id,
                    assistant_message_id,
                    &mut text,
                    delta,
                )
                .await?
                {
                    return finish_interrupted(FinishModelTurnInput {
                        event_tx: &event_tx,
                        turn_id,
                        assistant_message_id,
                        content,
                        text,
                        thinking,
                        usage,
                        tool_calls,
                        stop_reason: StopReason::Aborted,
                        cancellation: cancellation.clone(),
                    })
                    .await;
                }
            }
            ModelStreamEvent::TextEnd {
                text: final_text, ..
            } => {
                text = final_text;
            }
            ModelStreamEvent::ThinkingStart { .. } => {}
            ModelStreamEvent::ThinkingDelta { delta, .. } => {
                thinking.push_str(&delta);
                if !send_event(
                    &event_tx,
                    &cancellation,
                    CoreEvent::MessageDelta {
                        turn_id: turn_id.0,
                        message_id: assistant_message_id.0,
                        delta: MessageDelta::Thinking(delta),
                    },
                )
                .await?
                {
                    return finish_interrupted(FinishModelTurnInput {
                        event_tx: &event_tx,
                        turn_id,
                        assistant_message_id,
                        content,
                        text,
                        thinking,
                        usage,
                        tool_calls,
                        stop_reason: StopReason::Aborted,
                        cancellation: cancellation.clone(),
                    })
                    .await;
                }
            }
            ModelStreamEvent::ThinkingEnd {
                text: final_thinking,
                ..
            } => {
                thinking = final_thinking;
            }
            ModelStreamEvent::ToolCallStart {
                content_index,
                id,
                name,
            } => {
                let is_new = handle_tool_call_start(
                    &mut content,
                    &mut text,
                    &mut thinking,
                    &mut tool_calls,
                    content_index,
                    id.clone(),
                    name.clone(),
                )?;
                if is_new
                    && !send_event(
                        &event_tx,
                        &cancellation,
                        CoreEvent::MessageDelta {
                            turn_id: turn_id.0,
                            message_id: assistant_message_id.0,
                            delta: MessageDelta::ToolCall(ToolCallDelta::Start {
                                content_index,
                                tool_call_id: id,
                                tool_name: name,
                            }),
                        },
                    )
                    .await?
                {
                    return finish_interrupted(FinishModelTurnInput {
                        event_tx: &event_tx,
                        turn_id,
                        assistant_message_id,
                        content,
                        text,
                        thinking,
                        usage,
                        tool_calls,
                        stop_reason: StopReason::Aborted,
                        cancellation: cancellation.clone(),
                    })
                    .await;
                }
            }
            ModelStreamEvent::ToolCallDelta {
                content_index,
                id,
                name,
                arguments_delta,
            } => {
                let is_new = handle_tool_call_delta(
                    &mut content,
                    &mut text,
                    &mut thinking,
                    &mut tool_calls,
                    content_index,
                    id.clone(),
                    name.clone(),
                    &arguments_delta,
                )?;
                if is_new
                    && !send_event(
                        &event_tx,
                        &cancellation,
                        CoreEvent::MessageDelta {
                            turn_id: turn_id.0,
                            message_id: assistant_message_id.0,
                            delta: MessageDelta::ToolCall(ToolCallDelta::Start {
                                content_index,
                                tool_call_id: id.clone(),
                                tool_name: name.clone(),
                            }),
                        },
                    )
                    .await?
                {
                    return finish_interrupted(FinishModelTurnInput {
                        event_tx: &event_tx,
                        turn_id,
                        assistant_message_id,
                        content,
                        text,
                        thinking,
                        usage,
                        tool_calls,
                        stop_reason: StopReason::Aborted,
                        cancellation: cancellation.clone(),
                    })
                    .await;
                }
                if !send_event(
                    &event_tx,
                    &cancellation,
                    CoreEvent::MessageDelta {
                        turn_id: turn_id.0,
                        message_id: assistant_message_id.0,
                        delta: MessageDelta::ToolCall(ToolCallDelta::Arguments {
                            content_index,
                            tool_call_id: id,
                            tool_name: name,
                            delta: arguments_delta,
                        }),
                    },
                )
                .await?
                {
                    return finish_interrupted(FinishModelTurnInput {
                        event_tx: &event_tx,
                        turn_id,
                        assistant_message_id,
                        content,
                        text,
                        thinking,
                        usage,
                        tool_calls,
                        stop_reason: StopReason::Aborted,
                        cancellation: cancellation.clone(),
                    })
                    .await;
                }
            }
            ModelStreamEvent::ToolCallEnd {
                content_index,
                id,
                name,
                arguments,
            } => {
                handle_tool_call_end(
                    &mut content,
                    &mut tool_calls,
                    content_index,
                    id.clone(),
                    name.clone(),
                    arguments.clone(),
                )?;
                if !send_event(
                    &event_tx,
                    &cancellation,
                    CoreEvent::MessageDelta {
                        turn_id: turn_id.0,
                        message_id: assistant_message_id.0,
                        delta: MessageDelta::ToolCall(ToolCallDelta::End {
                            content_index,
                            tool_call_id: id,
                            tool_name: name,
                            arguments,
                        }),
                    },
                )
                .await?
                {
                    return finish_interrupted(FinishModelTurnInput {
                        event_tx: &event_tx,
                        turn_id,
                        assistant_message_id,
                        content,
                        text,
                        thinking,
                        usage,
                        tool_calls,
                        stop_reason: StopReason::Aborted,
                        cancellation: cancellation.clone(),
                    })
                    .await;
                }
            }
            ModelStreamEvent::Usage {
                usage: stream_usage,
            } => {
                usage = Some(stream_usage);
            }
            ModelStreamEvent::Done { stop_reason } => {
                return finish_model_turn(FinishModelTurnInput {
                    event_tx: &event_tx,
                    turn_id,
                    assistant_message_id,
                    content,
                    text,
                    thinking,
                    usage,
                    tool_calls,
                    stop_reason,
                    cancellation: cancellation.clone(),
                })
                .await;
            }
            ModelStreamEvent::Error { message } => {
                bail!(message);
            }
        }
    }

    bail!("model stream ended unexpectedly")
}

async fn handle_text_delta(
    event_tx: &mpsc::Sender<CoreEvent>,
    cancellation: &CancellationToken,
    turn_id: TurnId,
    assistant_message_id: MessageId,
    text: &mut String,
    delta: String,
) -> anyhow::Result<bool> {
    text.push_str(&delta);
    send_event(
        event_tx,
        cancellation,
        CoreEvent::MessageDelta {
            turn_id: turn_id.0,
            message_id: assistant_message_id.0,
            delta: MessageDelta::Text(delta),
        },
    )
    .await
}

/// Sends a core event to the frontend, honoring the turn's cancellation token.
///
/// The frontend event channel is bounded, so under a heavy token burst the TUI can
/// temporarily fall behind and a plain `send` would block. Because such a block sits
/// outside the loop's cancellation `select!`, it could delay an Esc-interrupt until the
/// backlog drains. Racing the send against the token keeps cancellation observable even
/// when the channel is full, so an interrupt is honored promptly during thinking/text.
///
/// Returns `Ok(true)` if the event was sent, `Ok(false)` if the turn was cancelled while
/// trying to send (the caller should finalize its partial reply and stop).
async fn send_event(
    event_tx: &mpsc::Sender<CoreEvent>,
    cancellation: &CancellationToken,
    event: CoreEvent,
) -> anyhow::Result<bool> {
    tokio::select! {
        () = cancellation.cancelled() => Ok(false),
        result = event_tx.send(event) => {
            result.context("failed to emit assistant message event")?;
            Ok(true)
        }
    }
}

fn handle_tool_call_start(
    content: &mut Vec<ContentBlock>,
    text: &mut String,
    thinking: &mut String,
    tool_calls: &mut ToolCallAssembler,
    content_index: usize,
    id: Option<String>,
    name: Option<String>,
) -> anyhow::Result<bool> {
    flush_text_and_thinking(content, text, thinking);
    tool_calls.start(content_index, id, name)
}

fn handle_tool_call_delta(
    content: &mut Vec<ContentBlock>,
    text: &mut String,
    thinking: &mut String,
    tool_calls: &mut ToolCallAssembler,
    content_index: usize,
    id: Option<String>,
    name: Option<String>,
    arguments_delta: &str,
) -> anyhow::Result<bool> {
    let is_new = tool_calls.push_arguments_delta(content_index, id, name, arguments_delta)?;
    if is_new {
        flush_text_and_thinking(content, text, thinking);
    }
    Ok(is_new)
}

fn handle_tool_call_end(
    content: &mut Vec<ContentBlock>,
    tool_calls: &mut ToolCallAssembler,
    content_index: usize,
    id: String,
    name: String,
    arguments: serde_json::Value,
) -> anyhow::Result<()> {
    tool_calls.finish(content_index, id, name, Some(arguments.clone()))?;
    let (id, name) = tool_calls.identity(content_index)?;
    content.push(ContentBlock::ToolCall {
        id,
        name,
        arguments,
    });
    Ok(())
}

struct FinishModelTurnInput<'a> {
    event_tx: &'a mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    assistant_message_id: MessageId,
    content: Vec<ContentBlock>,
    text: String,
    thinking: String,
    usage: Option<Usage>,
    tool_calls: ToolCallAssembler,
    stop_reason: StopReason,
    cancellation: CancellationToken,
}

async fn finish_model_turn(input: FinishModelTurnInput<'_>) -> anyhow::Result<ModelTurnOutcome> {
    let FinishModelTurnInput {
        event_tx,
        turn_id,
        assistant_message_id,
        content,
        text,
        thinking,
        usage,
        tool_calls,
        stop_reason,
        cancellation,
    } = input;

    let content = flush_content_and_emit_finished(
        event_tx,
        &cancellation,
        turn_id,
        assistant_message_id,
        content,
        text,
        thinking,
    )
    .await?;

    Ok(ModelTurnOutcome::Completed {
        assistant_message: AgentMessage::Assistant {
            id: assistant_message_id,
            content,
            usage,
            stop_reason: Some(stop_reason),
            timestamp: SystemTime::now(),
        },
        tool_calls: tool_calls.finish_all()?,
        stop_reason,
    })
}

async fn finish_interrupted(input: FinishModelTurnInput<'_>) -> anyhow::Result<ModelTurnOutcome> {
    let FinishModelTurnInput {
        event_tx,
        turn_id,
        assistant_message_id,
        content,
        text,
        thinking,
        usage,
        cancellation,
        ..
    } = input;

    let content = flush_content_and_emit_finished(
        event_tx,
        &cancellation,
        turn_id,
        assistant_message_id,
        content,
        text,
        thinking,
    )
    .await?;

    Ok(ModelTurnOutcome::Interrupted {
        assistant_message: AgentMessage::Assistant {
            id: assistant_message_id,
            content,
            usage,
            stop_reason: Some(StopReason::Aborted),
            timestamp: SystemTime::now(),
        },
    })
}

/// Flushes accumulated text/thinking into content blocks and emits `MessageFinished`
/// so the transcript seals the assistant message. Shared by the normal-completion and
/// interruption finalizers so a partial reply is sealed identically in both cases.
async fn flush_content_and_emit_finished(
    event_tx: &mpsc::Sender<CoreEvent>,
    cancellation: &CancellationToken,
    turn_id: TurnId,
    assistant_message_id: MessageId,
    mut content: Vec<ContentBlock>,
    mut text: String,
    mut thinking: String,
) -> anyhow::Result<Vec<ContentBlock>> {
    flush_text_and_thinking(&mut content, &mut text, &mut thinking);
    // Best-effort, cancellation-aware: the assistant's `MessageFinished` is not what
    // finalizes the message in the UI (TurnFinished/TurnCancelled are), so if the turn
    // is being interrupted we can skip it rather than stall on a full channel.
    send_event(
        event_tx,
        cancellation,
        CoreEvent::MessageFinished {
            turn_id: turn_id.0,
            message_id: assistant_message_id.0,
        },
    )
    .await?;
    Ok(content)
}

fn flush_text_and_thinking(
    content: &mut Vec<ContentBlock>,
    text: &mut String,
    thinking: &mut String,
) {
    if !thinking.is_empty() {
        content.push(ContentBlock::Thinking {
            text: std::mem::take(thinking),
        });
    }
    if !text.is_empty() {
        content.push(ContentBlock::Text {
            text: std::mem::take(text),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::stream::iter;
    use iyon_api::{
        ContentBlock, ModelApi, ModelRequest, ModelStream, ModelStreamEvent, ModelStreamFuture,
        StopReason,
    };

    use super::{ModelTurnInput, ModelTurnOutcome, run_model_turn};
    use crate::agent::tool_call::ToolCallRequest;
    use crate::ids::{MessageId, TurnId};
    use crate::{CoreEvent, MessageDelta, ToolCallDelta};

    struct ScriptedModel {
        events: Vec<ModelStreamEvent>,
    }

    impl ModelApi for ScriptedModel {
        fn stream(&self, _request: ModelRequest) -> ModelStreamFuture<'_> {
            let events = self.events.clone();
            Box::pin(async move {
                let stream: ModelStream = Box::pin(iter(events.into_iter().map(Ok)));
                Ok(stream)
            })
        }
    }

    /// A model whose stream is driven by a test-owned channel (`tx`), giving the
    /// test deterministic control over *when* each event is observed so it can send
    /// interrupt/steer controls mid-stream. Each `DrivenModel` supports one stream.
    struct DrivenModel {
        rx: Arc<std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<ModelStreamEvent>>>>,
    }

    impl ModelApi for DrivenModel {
        fn stream(&self, _request: ModelRequest) -> ModelStreamFuture<'_> {
            let rx = Arc::clone(&self.rx);
            Box::pin(async move {
                let rx = rx
                    .lock()
                    .unwrap()
                    .take()
                    .expect("DrivenModel supports a single stream");
                let stream: ModelStream =
                    Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
                        rx.recv().await.map(|event| (Ok(event), rx))
                    }));
                Ok(stream)
            })
        }
    }

    fn driven_model_events() -> (
        Arc<DrivenModel>,
        tokio::sync::mpsc::Sender<ModelStreamEvent>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let model = Arc::new(DrivenModel {
            rx: Arc::new(std::sync::Mutex::new(Some(rx))),
        });
        (model, tx)
    }

    fn model_events() -> Vec<ModelStreamEvent> {
        vec![
            ModelStreamEvent::Started,
            ModelStreamEvent::TextStart { content_index: 0 },
            ModelStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "think1".to_string(),
            },
            ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "hi ".to_string(),
            },
            ModelStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "think2".to_string(),
            },
            ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "there".to_string(),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::Stop,
            },
        ]
    }

    async fn run_scripted_turn(
        events: Vec<ModelStreamEvent>,
    ) -> (ModelTurnOutcome, Vec<CoreEvent>) {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        let model = Arc::new(ScriptedModel { events });
        let outcome = run_model_turn(ModelTurnInput {
            turn_id: TurnId(7),
            assistant_message_id: MessageId(9),
            request: ModelRequest::default(),
            model,
            event_tx: event_tx.clone(),
            cancellation: tokio_util::sync::CancellationToken::new(),
        })
        .await
        .expect("turn should succeed");
        drop(event_tx);

        let events = drain_events(&mut event_rx);
        (outcome, events)
    }

    fn message_deltas(events: &[CoreEvent]) -> Vec<&MessageDelta> {
        events
            .iter()
            .filter_map(|event| match event {
                CoreEvent::MessageDelta { delta, .. } => Some(delta),
                _ => None,
            })
            .collect()
    }

    fn drain_events(event_rx: &mut tokio::sync::mpsc::Receiver<CoreEvent>) -> Vec<CoreEvent> {
        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    fn assert_no_tool_call_started(events: &[CoreEvent]) {
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, CoreEvent::ToolCallStarted { .. }))
        );
    }

    #[tokio::test]
    async fn thinking_deltas_are_streamed_as_core_events_in_order() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        let model = Arc::new(ScriptedModel {
            events: model_events(),
        });

        let outcome = run_model_turn(ModelTurnInput {
            turn_id: TurnId(7),
            assistant_message_id: MessageId(9),
            request: ModelRequest::default(),
            model,
            event_tx: event_tx.clone(),
            cancellation: tokio_util::sync::CancellationToken::new(),
        })
        .await
        .expect("turn should succeed");

        assert!(matches!(outcome, ModelTurnOutcome::Completed { .. }));

        drop(event_tx);
        let mut deltas: Vec<String> = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            if let crate::CoreEvent::MessageDelta { delta, .. } = event {
                match delta {
                    crate::MessageDelta::Thinking(text) => deltas.push(format!("T:{text}")),
                    crate::MessageDelta::Text(text) => deltas.push(format!("X:{text}")),
                    crate::MessageDelta::ToolCall(_) => deltas.push("C".to_string()),
                }
            }
        }

        assert_eq!(deltas, vec!["T:think1", "X:hi ", "T:think2", "X:there"]);
    }

    #[tokio::test]
    async fn tool_call_start_is_streamed_before_done_without_execution_start() {
        let (outcome, events) = run_scripted_turn(vec![
            ModelStreamEvent::ToolCallStart {
                content_index: 0,
                id: Some("call-1".to_string()),
                name: Some("search".to_string()),
            },
            ModelStreamEvent::ToolCallEnd {
                content_index: 0,
                id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({"query": "iyon"}),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            },
        ])
        .await;

        let deltas = message_deltas(&events);
        assert!(matches!(
            deltas[0],
            MessageDelta::ToolCall(ToolCallDelta::Start { .. })
        ));
        assert!(matches!(
            deltas[1],
            MessageDelta::ToolCall(ToolCallDelta::End { .. })
        ));
        assert_no_tool_call_started(&events);
        assert!(matches!(outcome, ModelTurnOutcome::Completed { .. }));
    }

    #[tokio::test]
    async fn tool_call_argument_fragments_preserve_order_before_end() {
        let (_, events) = run_scripted_turn(vec![
            ModelStreamEvent::ToolCallStart {
                content_index: 1,
                id: Some("call-1".to_string()),
                name: Some("search".to_string()),
            },
            ModelStreamEvent::ToolCallDelta {
                content_index: 1,
                id: None,
                name: None,
                arguments_delta: "{\"q\":".to_string(),
            },
            ModelStreamEvent::ToolCallDelta {
                content_index: 1,
                id: None,
                name: None,
                arguments_delta: "\"iyon\"}".to_string(),
            },
            ModelStreamEvent::ToolCallEnd {
                content_index: 1,
                id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({"q": "iyon"}),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            },
        ])
        .await;

        let deltas = message_deltas(&events);
        assert!(matches!(
            deltas[0],
            MessageDelta::ToolCall(ToolCallDelta::Start { .. })
        ));
        assert!(matches!(
            deltas[1],
            MessageDelta::ToolCall(ToolCallDelta::Arguments { delta, .. }) if delta == "{\"q\":"
        ));
        assert!(matches!(
            deltas[2],
            MessageDelta::ToolCall(ToolCallDelta::Arguments { delta, .. }) if delta == "\"iyon\"}"
        ));
        assert!(matches!(
            deltas[3],
            MessageDelta::ToolCall(ToolCallDelta::End { .. })
        ));
    }

    #[tokio::test]
    async fn tool_call_end_emits_authoritative_identity_and_arguments() {
        let (outcome, events) = run_scripted_turn(vec![
            ModelStreamEvent::ToolCallStart {
                content_index: 2,
                id: None,
                name: Some("old-name".to_string()),
            },
            ModelStreamEvent::ToolCallEnd {
                content_index: 2,
                id: "authoritative-id".to_string(),
                name: "authoritative-name".to_string(),
                arguments: serde_json::json!({"value": 42}),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            },
        ])
        .await;

        let deltas = message_deltas(&events);
        let MessageDelta::ToolCall(ToolCallDelta::End {
            tool_call_id,
            tool_name,
            arguments,
            ..
        }) = deltas[1]
        else {
            panic!("expected tool call end");
        };
        assert_eq!(tool_call_id, "authoritative-id");
        assert_eq!(tool_name, "authoritative-name");
        assert_eq!(arguments, &serde_json::json!({"value": 42}));

        let ModelTurnOutcome::Completed { tool_calls, .. } = outcome else {
            panic!("expected completed turn");
        };
        let ToolCallRequest::Ready(call) = &tool_calls[0] else {
            panic!("expected ready tool call");
        };
        assert_eq!(call.id.0, "authoritative-id");
        assert_eq!(call.name, "authoritative-name");
        assert_eq!(call.arguments, serde_json::json!({"value": 42}));
    }

    #[tokio::test]
    async fn text_tool_text_event_order_is_preserved() {
        let (_, events) = run_scripted_turn(vec![
            ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "before".to_string(),
            },
            ModelStreamEvent::ToolCallStart {
                content_index: 1,
                id: Some("call-1".to_string()),
                name: Some("search".to_string()),
            },
            ModelStreamEvent::ToolCallDelta {
                content_index: 1,
                id: None,
                name: None,
                arguments_delta: "{}".to_string(),
            },
            ModelStreamEvent::ToolCallEnd {
                content_index: 1,
                id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({}),
            },
            ModelStreamEvent::TextDelta {
                content_index: 2,
                delta: "after".to_string(),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            },
        ])
        .await;

        let deltas = message_deltas(&events);
        assert!(matches!(deltas[0], MessageDelta::Text(text) if text == "before"));
        assert!(matches!(
            deltas[1],
            MessageDelta::ToolCall(ToolCallDelta::Start { .. })
        ));
        assert!(matches!(
            deltas[2],
            MessageDelta::ToolCall(ToolCallDelta::Arguments { delta, .. }) if delta == "{}"
        ));
        assert!(matches!(
            deltas[3],
            MessageDelta::ToolCall(ToolCallDelta::End { .. })
        ));
        assert!(matches!(deltas[4], MessageDelta::Text(text) if text == "after"));
    }

    #[tokio::test]
    async fn thinking_is_flushed_before_tool_call_boundary() {
        let (outcome, events) = run_scripted_turn(vec![
            ModelStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "thinking".to_string(),
            },
            ModelStreamEvent::ToolCallStart {
                content_index: 1,
                id: Some("call-1".to_string()),
                name: Some("search".to_string()),
            },
            ModelStreamEvent::ToolCallEnd {
                content_index: 1,
                id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({}),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            },
        ])
        .await;

        let deltas = message_deltas(&events);
        assert!(matches!(deltas[0], MessageDelta::Thinking(text) if text == "thinking"));
        assert!(matches!(
            deltas[1],
            MessageDelta::ToolCall(ToolCallDelta::Start { .. })
        ));
        let ModelTurnOutcome::Completed {
            assistant_message, ..
        } = outcome
        else {
            panic!("expected completed turn");
        };
        let crate::agent::transcript::AgentMessage::Assistant { content, .. } = assistant_message
        else {
            panic!("expected assistant message");
        };
        assert!(matches!(
            &content[0],
            ContentBlock::Thinking { text } if text == "thinking"
        ));
        assert!(matches!(&content[1], ContentBlock::ToolCall { .. }));
    }

    #[tokio::test]
    async fn delta_before_start_emits_one_draft_and_one_request() {
        let (outcome, events) = run_scripted_turn(vec![
            ModelStreamEvent::ToolCallDelta {
                content_index: 3,
                id: Some("call-3".to_string()),
                name: Some("search".to_string()),
                arguments_delta: "{}".to_string(),
            },
            ModelStreamEvent::ToolCallStart {
                content_index: 3,
                id: None,
                name: None,
            },
            ModelStreamEvent::ToolCallEnd {
                content_index: 3,
                id: "call-3".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({}),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            },
        ])
        .await;

        let deltas = message_deltas(&events);
        assert_eq!(
            deltas
                .iter()
                .filter(|delta| {
                    matches!(delta, MessageDelta::ToolCall(ToolCallDelta::Start { .. }))
                })
                .count(),
            1
        );
        assert!(matches!(
            deltas[0],
            MessageDelta::ToolCall(ToolCallDelta::Start { .. })
        ));
        assert!(matches!(
            deltas[1],
            MessageDelta::ToolCall(ToolCallDelta::Arguments { .. })
        ));
        let ModelTurnOutcome::Completed { tool_calls, .. } = outcome else {
            panic!("expected completed turn");
        };
        assert_eq!(tool_calls.len(), 1);
    }

    #[tokio::test]
    async fn cancellation_after_tool_start_preserves_partial_message() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
        let (model, tx) = driven_model_events();
        let cancellation = tokio_util::sync::CancellationToken::new();
        let cancel_token = cancellation.clone();
        let driver = tokio::spawn(async move {
            tx.send(ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "partial".to_string(),
            })
            .await
            .unwrap();
            tx.send(ModelStreamEvent::ToolCallStart {
                content_index: 1,
                id: Some("call-1".to_string()),
                name: Some("search".to_string()),
            })
            .await
            .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_token.cancel();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });

        let outcome = run_model_turn(ModelTurnInput {
            turn_id: TurnId(8),
            assistant_message_id: MessageId(10),
            request: ModelRequest::default(),
            model,
            event_tx: event_tx.clone(),
            cancellation,
        })
        .await
        .expect("turn should interrupt cleanly");
        driver.await.unwrap();
        drop(event_tx);
        let events = drain_events(&mut event_rx);

        let ModelTurnOutcome::Interrupted { assistant_message } = outcome else {
            panic!("expected interrupted turn");
        };
        let crate::agent::transcript::AgentMessage::Assistant { content, .. } = assistant_message
        else {
            panic!("expected assistant message");
        };
        assert!(content.iter().any(|block| matches!(
            block,
            ContentBlock::Text { text } if text == "partial"
        )));
        assert!(
            message_deltas(&events)
                .iter()
                .any(|delta| matches!(delta, MessageDelta::ToolCall(ToolCallDelta::Start { .. })))
        );
        assert_no_tool_call_started(&events);
    }

    #[tokio::test]
    async fn cancel_preserves_partial_text_and_thinking() {
        let (event_tx, _event_rx) = tokio::sync::mpsc::channel(64);
        let (model, tx) = driven_model_events();
        let cancellation = tokio_util::sync::CancellationToken::new();
        let cancel_token = cancellation.clone();

        // Drive the stream and fire the Esc-interrupt (cancellation token) from a
        // spawned task while the test task awaits the turn directly. The small sleeps
        // let the turn drain already-queued deltas before the token fires, so the
        // partial reply is preserved at interrupt time.
        let driver_tx = tx.clone();
        let driver = tokio::spawn(async move {
            driver_tx.send(ModelStreamEvent::Started).await.unwrap();
            driver_tx
                .send(ModelStreamEvent::TextStart { content_index: 0 })
                .await
                .unwrap();
            driver_tx
                .send(ModelStreamEvent::ThinkingDelta {
                    content_index: 0,
                    delta: "a thought".to_string(),
                })
                .await
                .unwrap();
            driver_tx
                .send(ModelStreamEvent::TextDelta {
                    content_index: 0,
                    delta: "partial".to_string(),
                })
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_token.cancel();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            drop(driver_tx);
        });

        let outcome = run_model_turn(ModelTurnInput {
            turn_id: TurnId(3),
            assistant_message_id: MessageId(5),
            request: ModelRequest::default(),
            model,
            event_tx: event_tx.clone(),
            cancellation,
        })
        .await
        .expect("turn should return cleanly");
        driver.await.unwrap();

        let ModelTurnOutcome::Interrupted { assistant_message } = outcome else {
            panic!("expected interrupted outcome");
        };

        use iyon_api::ContentBlock;
        let crate::agent::transcript::AgentMessage::Assistant { content, .. } = &assistant_message
        else {
            panic!("expected assistant message");
        };
        let text: Vec<String> = content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => text.clone(),
                ContentBlock::Thinking { text } => text.clone(),
                ContentBlock::ToolCall { .. } | ContentBlock::Image { .. } => String::new(),
            })
            .collect();
        assert!(text.iter().any(|t| t.contains("partial")), "text: {text:?}");
        assert!(
            text.iter().any(|t| t.contains("a thought")),
            "text: {text:?}"
        );
    }

    /// Esc-interrupt must be honored promptly even when the (bounded) frontend event
    /// channel is full. Previously a plain `event_tx.send().await` inside the stream
    /// handler would block there (outside the cancellation `select!`), so an interrupt
    /// under heavy thinking/text backpressure stalled instead of cancelling. This test
    /// fills a tiny event channel and verifies cancellation still resolves the turn.
    #[tokio::test]
    async fn cancellation_honored_under_event_backpressure() {
        // Tiny, never-drained channel → the event sender blocks after a couple of sends.
        let (event_tx, _event_rx) = tokio::sync::mpsc::channel(2);
        let (model, tx) = driven_model_events();
        let cancellation = tokio_util::sync::CancellationToken::new();
        let cancel_token = cancellation.clone();

        let driver_tx = tx.clone();
        let driver = tokio::spawn(async move {
            driver_tx.send(ModelStreamEvent::Started).await.unwrap();
            driver_tx
                .send(ModelStreamEvent::TextDelta {
                    content_index: 0,
                    delta: "x".to_string(),
                })
                .await
                .unwrap();
            driver_tx
                .send(ModelStreamEvent::TextDelta {
                    content_index: 0,
                    delta: "y".to_string(),
                })
                .await
                .unwrap();
            // Let the turn process deltas until its event send blocks on the full
            // channel, then interrupt.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_token.cancel();
            drop(driver_tx);
        });

        let outcome = run_model_turn(ModelTurnInput {
            turn_id: TurnId(21),
            assistant_message_id: MessageId(22),
            request: ModelRequest::default(),
            model,
            event_tx: event_tx.clone(),
            cancellation,
        })
        .await
        .expect("interrupt under backpressure should resolve the turn cleanly");
        driver.await.unwrap();

        assert!(
            matches!(outcome, ModelTurnOutcome::Interrupted { .. }),
            "expected interrupted under backpressure, got {outcome:?}"
        );
    }
}
