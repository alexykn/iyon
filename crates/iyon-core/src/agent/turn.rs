use std::{sync::Arc, time::SystemTime};

use anyhow::{Context, bail};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreEvent, MessageDelta, MessageRole,
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
            } => handle_tool_call_start(
                &mut content,
                &mut text,
                &mut thinking,
                &mut tool_calls,
                content_index,
                id,
                name,
            )?,
            ModelStreamEvent::ToolCallDelta {
                content_index,
                id,
                name,
                arguments_delta,
            } => handle_tool_call_delta(&mut tool_calls, content_index, id, name, arguments_delta)?,
            ModelStreamEvent::ToolCallEnd {
                content_index,
                id,
                name,
                arguments,
            } => handle_tool_call_end(
                &mut content,
                &mut tool_calls,
                content_index,
                id,
                name,
                arguments,
            )?,
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
) -> anyhow::Result<()> {
    flush_text_and_thinking(content, text, thinking);
    let id = id.unwrap_or_else(|| generated_tool_call_id(content_index));
    tool_calls.start(id, name.unwrap_or_default())
}

fn handle_tool_call_delta(
    tool_calls: &mut ToolCallAssembler,
    content_index: usize,
    id: Option<String>,
    name: Option<String>,
    arguments_delta: String,
) -> anyhow::Result<()> {
    let id = id.unwrap_or_else(|| generated_tool_call_id(content_index));
    if tool_calls
        .push_arguments_delta(&id, &arguments_delta)
        .is_err()
    {
        tool_calls.start(id.clone(), name.unwrap_or_default())?;
        tool_calls.push_arguments_delta(&id, &arguments_delta)?;
    }
    Ok(())
}

fn handle_tool_call_end(
    content: &mut Vec<ContentBlock>,
    tool_calls: &mut ToolCallAssembler,
    content_index: usize,
    id: String,
    name: String,
    arguments: serde_json::Value,
) -> anyhow::Result<()> {
    if tool_calls.finish(&id, Some(arguments.clone())).is_err() {
        tool_calls.start(id.clone(), name.clone())?;
        tool_calls.finish(&id, Some(arguments.clone()))?;
    }
    content.push(ContentBlock::ToolCall {
        id: non_empty_tool_call_id(id, content_index),
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

fn non_empty_tool_call_id(id: String, content_index: usize) -> String {
    if id.is_empty() {
        generated_tool_call_id(content_index)
    } else {
        id
    }
}

fn generated_tool_call_id(content_index: usize) -> String {
    format!("tool_call_{content_index}")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::stream::iter;
    use iyon_api::{
        ModelApi, ModelRequest, ModelStream, ModelStreamEvent, ModelStreamFuture, StopReason,
    };

    use super::{ModelTurnInput, ModelTurnOutcome, run_model_turn};
    use crate::ids::{MessageId, TurnId};

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
                let stream: ModelStream = Box::pin(futures_util::stream::unfold(rx, |mut rx| {
                    async move { rx.recv().await.map(|event| (Ok(event), rx)) }
                }));
                Ok(stream)
            })
        }
    }

    fn driven_model_events() -> (Arc<DrivenModel>, tokio::sync::mpsc::Sender<ModelStreamEvent>) {
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
                    crate::MessageDelta::ToolCall { .. } => deltas.push("C".to_string()),
                }
            }
        }

        assert_eq!(
            deltas,
            vec!["T:think1", "X:hi ", "T:think2", "X:there"]
        );
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
        assert!(text.iter().any(|t| t.contains("a thought")), "text: {text:?}");
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
