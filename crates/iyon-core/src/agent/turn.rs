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
pub(crate) struct ModelTurnOutcome {
    pub assistant_message: AgentMessage,
    pub tool_calls: Vec<ToolCallRequest>,
    pub stop_reason: StopReason,
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

    event_tx
        .send(CoreEvent::MessageStarted {
            turn_id: turn_id.0,
            message_id: assistant_message_id.0,
            role: MessageRole::Assistant,
        })
        .await
        .context("failed to emit assistant message start")?;

    let mut content = Vec::new();
    let mut text = String::new();
    let mut thinking = String::new();
    let mut usage: Option<Usage> = None;
    let mut tool_calls = ToolCallAssembler::default();

    loop {
        let event = tokio::select! {
            () = cancellation.cancelled() => bail!("turn cancelled"),
            event = stream.next() => event,
        };
        let Some(event) = event else {
            break;
        };
        match event.context("model stream error")? {
            ModelStreamEvent::Started | ModelStreamEvent::TextStart { .. } => {}
            ModelStreamEvent::TextDelta { delta, .. } => {
                handle_text_delta(&event_tx, turn_id, assistant_message_id, &mut text, delta)
                    .await?;
            }
            ModelStreamEvent::TextEnd {
                text: final_text, ..
            } => {
                text = final_text;
            }
            ModelStreamEvent::ThinkingStart { .. } => {}
            ModelStreamEvent::ThinkingDelta { delta, .. } => {
                thinking.push_str(&delta);
                event_tx
                    .send(CoreEvent::MessageDelta {
                        turn_id: turn_id.0,
                        message_id: assistant_message_id.0,
                        delta: MessageDelta::Thinking(delta),
                    })
                    .await
                    .context("failed to emit assistant thinking delta")?;
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
    turn_id: TurnId,
    assistant_message_id: MessageId,
    text: &mut String,
    delta: String,
) -> anyhow::Result<()> {
    text.push_str(&delta);
    event_tx
        .send(CoreEvent::MessageDelta {
            turn_id: turn_id.0,
            message_id: assistant_message_id.0,
            delta: MessageDelta::Text(delta),
        })
        .await
        .context("failed to emit assistant text delta")
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
}

async fn finish_model_turn(input: FinishModelTurnInput<'_>) -> anyhow::Result<ModelTurnOutcome> {
    let FinishModelTurnInput {
        event_tx,
        turn_id,
        assistant_message_id,
        mut content,
        mut text,
        mut thinking,
        usage,
        tool_calls,
        stop_reason,
    } = input;

    flush_text_and_thinking(&mut content, &mut text, &mut thinking);
    event_tx
        .send(CoreEvent::MessageFinished {
            turn_id: turn_id.0,
            message_id: assistant_message_id.0,
        })
        .await
        .context("failed to emit assistant message finish")?;

    Ok(ModelTurnOutcome {
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

    use super::{ModelTurnInput, run_model_turn};
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

        assert_eq!(outcome.stop_reason, StopReason::Stop);

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
}
