use std::time::SystemTime;

use iyon_api::{ContentBlock, ModelStreamEvent, StopReason, Usage};
use thiserror::Error;

use crate::{
    CoreEvent, MessageDelta, MessageRole, ToolCallDelta,
    agent::{
        tool_call::{ToolCallAssembler, ToolCallRequest},
        transcript::AgentMessage,
    },
    ids::{MessageId, TurnId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTurnState {
    Active,
    Finished,
    Cancelled,
    Failed,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ModelTurnError {
    #[error("model turn is no longer active ({0:?})")]
    InvalidState(ModelTurnState),
    #[error("provider stream error: {0}")]
    Provider(String),
    #[error("model turn did not receive a stop reason")]
    MissingStopReason,
    #[error("model turn could not assemble tool calls: {0}")]
    Assembly(String),
}

#[derive(Debug, Clone)]
pub struct ModelTurnResult {
    pub turn_id: TurnId,
    pub assistant_message: AgentMessage,
    pub tool_calls: Vec<ToolCallRequest>,
    pub stop_reason: StopReason,
    pub cancelled: bool,
}

pub struct ModelTurn {
    turn_id: TurnId,
    assistant_message_id: MessageId,
    state: ModelTurnState,
    content: Vec<ContentBlock>,
    text: String,
    thinking: String,
    usage: Option<Usage>,
    stop_reason: Option<StopReason>,
    tool_calls: ToolCallAssembler,
    events: Vec<CoreEvent>,
    failure: Option<String>,
}

impl ModelTurn {
    pub fn begin(turn_id: TurnId, assistant_message_id: MessageId) -> Self {
        Self::new(turn_id, assistant_message_id)
    }

    pub fn new(turn_id: TurnId, assistant_message_id: MessageId) -> Self {
        Self {
            turn_id,
            assistant_message_id,
            state: ModelTurnState::Active,
            content: Vec::new(),
            text: String::new(),
            thinking: String::new(),
            usage: None,
            stop_reason: None,
            tool_calls: ToolCallAssembler::default(),
            events: vec![CoreEvent::MessageStarted {
                turn_id: turn_id.0,
                message_id: assistant_message_id.0,
                role: MessageRole::Assistant,
            }],
            failure: None,
        }
    }

    pub fn turn_id(&self) -> TurnId {
        self.turn_id
    }

    pub fn assistant_message_id(&self) -> MessageId {
        self.assistant_message_id
    }

    pub fn state(&self) -> ModelTurnState {
        self.state
    }

    pub fn events(&self) -> &[CoreEvent] {
        &self.events
    }

    pub fn take_events(&mut self) -> Vec<CoreEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn push(&mut self, event: ModelStreamEvent) -> Result<(), ModelTurnError> {
        self.ensure_active()?;
        match event {
            ModelStreamEvent::Started
            | ModelStreamEvent::TextStart { .. }
            | ModelStreamEvent::ThinkingStart { .. } => {}
            ModelStreamEvent::TextDelta { delta, .. } => {
                self.text.push_str(&delta);
                self.events.push(CoreEvent::MessageDelta {
                    turn_id: self.turn_id.0,
                    message_id: self.assistant_message_id.0,
                    delta: MessageDelta::Text(delta),
                });
            }
            ModelStreamEvent::TextEnd { text, .. } => self.text = text,
            ModelStreamEvent::ThinkingDelta { delta, .. } => {
                self.thinking.push_str(&delta);
                self.events.push(CoreEvent::MessageDelta {
                    turn_id: self.turn_id.0,
                    message_id: self.assistant_message_id.0,
                    delta: MessageDelta::Thinking(delta),
                });
            }
            ModelStreamEvent::ThinkingEnd { text, .. } => self.thinking = text,
            ModelStreamEvent::ToolCallStart {
                content_index,
                id,
                name,
            } => {
                self.flush_text_and_thinking();
                let is_new = self
                    .tool_calls
                    .start(content_index, id.clone(), name.clone())
                    .map_err(|error| ModelTurnError::Assembly(error.to_string()))?;
                if is_new {
                    self.events.push(CoreEvent::MessageDelta {
                        turn_id: self.turn_id.0,
                        message_id: self.assistant_message_id.0,
                        delta: MessageDelta::ToolCall(ToolCallDelta::Start {
                            content_index,
                            tool_call_id: id,
                            tool_name: name,
                        }),
                    });
                }
            }
            ModelStreamEvent::ToolCallDelta {
                content_index,
                id,
                name,
                arguments_delta,
            } => {
                let is_new = self
                    .tool_calls
                    .push_arguments_delta(content_index, id.clone(), name.clone(), &arguments_delta)
                    .map_err(|error| ModelTurnError::Assembly(error.to_string()))?;
                if is_new {
                    self.flush_text_and_thinking();
                    self.events.push(CoreEvent::MessageDelta {
                        turn_id: self.turn_id.0,
                        message_id: self.assistant_message_id.0,
                        delta: MessageDelta::ToolCall(ToolCallDelta::Start {
                            content_index,
                            tool_call_id: id.clone(),
                            tool_name: name.clone(),
                        }),
                    });
                }
                self.events.push(CoreEvent::MessageDelta {
                    turn_id: self.turn_id.0,
                    message_id: self.assistant_message_id.0,
                    delta: MessageDelta::ToolCall(ToolCallDelta::Arguments {
                        content_index,
                        tool_call_id: id,
                        tool_name: name,
                        delta: arguments_delta,
                    }),
                });
            }
            ModelStreamEvent::ToolCallEnd {
                content_index,
                id,
                name,
                arguments,
            } => {
                self.tool_calls
                    .finish(
                        content_index,
                        id.clone(),
                        name.clone(),
                        Some(arguments.clone()),
                    )
                    .map_err(|error| ModelTurnError::Assembly(error.to_string()))?;
                let (identity, tool_name) = self
                    .tool_calls
                    .identity(content_index)
                    .map_err(|error| ModelTurnError::Assembly(error.to_string()))?;
                self.content.push(ContentBlock::ToolCall {
                    id: identity.clone(),
                    name: tool_name.clone(),
                    arguments: arguments.clone(),
                });
                self.events.push(CoreEvent::MessageDelta {
                    turn_id: self.turn_id.0,
                    message_id: self.assistant_message_id.0,
                    delta: MessageDelta::ToolCall(ToolCallDelta::End {
                        content_index,
                        tool_call_id: identity,
                        tool_name,
                        arguments,
                    }),
                });
            }
            ModelStreamEvent::Usage { usage } => self.usage = Some(usage),
            ModelStreamEvent::Done { stop_reason } => self.stop_reason = Some(stop_reason),
            ModelStreamEvent::Error { message } => {
                self.fail(message.clone());
                return Err(ModelTurnError::Provider(message));
            }
        }
        Ok(())
    }

    pub fn push_many<I>(&mut self, events: I) -> Result<(), ModelTurnError>
    where
        I: IntoIterator<Item = ModelStreamEvent>,
    {
        for event in events {
            self.push(event)?;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<ModelTurnResult, ModelTurnError> {
        let stop_reason = self.stop_reason.ok_or(ModelTurnError::MissingStopReason)?;
        self.finish_with(stop_reason, false)
    }

    pub fn cancel(&mut self) -> Result<ModelTurnResult, ModelTurnError> {
        self.ensure_active()?;
        self.finish_with(StopReason::Aborted, true)
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = ModelTurnState::Failed;
        self.failure = Some(error.into());
    }

    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    fn finish_with(
        &mut self,
        stop_reason: StopReason,
        cancelled: bool,
    ) -> Result<ModelTurnResult, ModelTurnError> {
        self.ensure_active()?;
        self.flush_text_and_thinking();
        self.state = if cancelled {
            ModelTurnState::Cancelled
        } else {
            ModelTurnState::Finished
        };
        self.events.push(CoreEvent::MessageFinished {
            turn_id: self.turn_id.0,
            message_id: self.assistant_message_id.0,
        });
        let tool_calls = if cancelled {
            Vec::new()
        } else {
            self.tool_calls
                .clone()
                .finish_all()
                .map_err(|error| ModelTurnError::Assembly(error.to_string()))?
        };
        Ok(ModelTurnResult {
            turn_id: self.turn_id,
            assistant_message: AgentMessage::Assistant {
                id: self.assistant_message_id,
                content: self.content.clone(),
                usage: self.usage,
                stop_reason: Some(stop_reason),
                timestamp: SystemTime::now(),
            },
            tool_calls,
            stop_reason,
            cancelled,
        })
    }

    fn ensure_active(&self) -> Result<(), ModelTurnError> {
        if self.state == ModelTurnState::Active {
            return Ok(());
        }
        Err(ModelTurnError::InvalidState(self.state))
    }

    fn flush_text_and_thinking(&mut self) {
        if !self.thinking.is_empty() {
            self.content.push(ContentBlock::Thinking {
                text: std::mem::take(&mut self.thinking),
            });
        }
        if !self.text.is_empty() {
            self.content.push(ContentBlock::Text {
                text: std::mem::take(&mut self.text),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use iyon_api::{ContentBlock, ModelStreamEvent, StopReason, Usage};

    use super::{ModelTurn, ModelTurnState};
    use crate::ids::{MessageId, TurnId};
    use crate::{CoreEvent, MessageDelta, ToolCallDelta};

    fn turn() -> ModelTurn {
        ModelTurn::new(TurnId(3), MessageId(8))
    }

    #[test]
    fn push_many_preserves_event_order() {
        let mut turn = turn();
        turn.push_many([
            ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "a".into(),
            },
            ModelStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "b".into(),
            },
        ])
        .unwrap();
        assert!(
            matches!(turn.events()[1], CoreEvent::MessageDelta { delta: MessageDelta::Text(ref value), .. } if value == "a")
        );
        assert!(
            matches!(turn.events()[2], CoreEvent::MessageDelta { delta: MessageDelta::Thinking(ref value), .. } if value == "b")
        );
    }

    #[test]
    fn push_accumulates_text_and_thinking() {
        let mut turn = turn();
        turn.push_many([
            ModelStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "plan".into(),
            },
            ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "answer".into(),
            },
            ModelStreamEvent::Done {
                stop_reason: StopReason::Stop,
            },
        ])
        .unwrap();
        let result = turn.finish().unwrap();
        let super::super::AgentMessage::Assistant { content, .. } = result.assistant_message else {
            panic!()
        };
        assert!(matches!(&content[0], ContentBlock::Thinking { text } if text == "plan"));
        assert!(matches!(&content[1], ContentBlock::Text { text } if text == "answer"));
    }

    #[test]
    fn tool_draft_identity_survives_late_provider_id() {
        let mut turn = turn();
        turn.push(ModelStreamEvent::ToolCallStart {
            content_index: 2,
            id: None,
            name: Some("read".into()),
        })
        .unwrap();
        turn.push(ModelStreamEvent::ToolCallDelta {
            content_index: 2,
            id: None,
            name: None,
            arguments_delta: "{}".into(),
        })
        .unwrap();
        turn.push(ModelStreamEvent::ToolCallEnd {
            content_index: 2,
            id: "provider-2".into(),
            name: "read".into(),
            arguments: serde_json::json!({}),
        })
        .unwrap();
        let result = turn.finish_with_test_stop().unwrap();
        assert!(
            matches!(result.tool_calls[0], crate::kernel::ToolCallRequest::Ready(ref call) if call.id.0 == "provider-2")
        );
        let starts = turn
            .events()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    CoreEvent::MessageDelta {
                        delta: MessageDelta::ToolCall(ToolCallDelta::Start { .. }),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(starts, 1);
    }

    #[test]
    fn delta_before_start_emits_one_draft() {
        let mut turn = turn();
        turn.push(ModelStreamEvent::ToolCallDelta {
            content_index: 4,
            id: Some("call-4".into()),
            name: Some("read".into()),
            arguments_delta: "{}".into(),
        })
        .unwrap();
        turn.push(ModelStreamEvent::ToolCallEnd {
            content_index: 4,
            id: "call-4".into(),
            name: "read".into(),
            arguments: serde_json::json!({}),
        })
        .unwrap();
        turn.finish_with_test_stop().unwrap();
        assert_eq!(
            turn.events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CoreEvent::MessageDelta {
                        delta: MessageDelta::ToolCall(ToolCallDelta::Start { .. }),
                        ..
                    }
                ))
                .count(),
            1
        );
    }

    #[test]
    fn finish_preserves_usage_and_assistant_id() {
        let mut turn = turn();
        turn.push(ModelStreamEvent::Usage {
            usage: Usage {
                input_tokens: 1,
                output_tokens: 2,
                cache_read_tokens: 3,
                cache_write_tokens: 4,
            },
        })
        .unwrap();
        turn.push(ModelStreamEvent::Done {
            stop_reason: StopReason::Stop,
        })
        .unwrap();
        let result = turn.finish().unwrap();
        assert_eq!(result.assistant_message.id(), MessageId(8));
        let super::super::AgentMessage::Assistant { usage, .. } = result.assistant_message else {
            panic!()
        };
        assert_eq!(usage.unwrap().output_tokens, 2);
    }

    #[test]
    fn cancel_preserves_partial_text_and_thinking() {
        let mut turn = turn();
        turn.push_many([
            ModelStreamEvent::ThinkingDelta {
                content_index: 0,
                delta: "partial plan".into(),
            },
            ModelStreamEvent::TextDelta {
                content_index: 0,
                delta: "partial answer".into(),
            },
        ])
        .unwrap();
        let result = turn.cancel().unwrap();
        assert_eq!(turn.state(), ModelTurnState::Cancelled);
        let super::super::AgentMessage::Assistant {
            content,
            stop_reason,
            ..
        } = result.assistant_message
        else {
            panic!()
        };
        assert_eq!(stop_reason, Some(StopReason::Aborted));
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn fail_does_not_fake_a_successful_result() {
        let mut turn = turn();
        turn.fail("provider failed");
        assert_eq!(turn.state(), ModelTurnState::Failed);
        assert!(turn.finish().is_err());
        assert_eq!(turn.failure(), Some("provider failed"));
    }

    #[test]
    fn cancellation_honored_under_event_backpressure() {
        let mut turn = turn();
        turn.push(ModelStreamEvent::TextDelta {
            content_index: 0,
            delta: "still-owned".into(),
        })
        .unwrap();
        let result = turn.cancel().unwrap();
        assert!(result.cancelled);
        assert!(
            turn.events()
                .iter()
                .any(|event| matches!(event, CoreEvent::MessageFinished { .. }))
        );
    }

    trait TestStop {
        fn finish_with_test_stop(
            &mut self,
        ) -> Result<super::super::ModelTurnResult, super::super::ModelTurnError>;
    }

    impl TestStop for ModelTurn {
        fn finish_with_test_stop(
            &mut self,
        ) -> Result<super::super::ModelTurnResult, super::super::ModelTurnError> {
            self.push(ModelStreamEvent::Done {
                stop_reason: StopReason::ToolUse,
            })
            .unwrap();
            self.finish()
        }
    }
}
