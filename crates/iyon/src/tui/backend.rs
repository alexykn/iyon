use std::{collections::HashMap, fmt};

use anyhow::Result;
use iyon_core::{
    CoreCommand, CoreCommandSender, CoreEvent, CoreEventReceiver, MessageDelta, MessageRole,
    ReasoningLevel, ToolCallDelta, ToolUpdateEvent,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use iyon_tui::AppHandle;

use super::controller::IyonAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToolDraftKey {
    pub message_id: u64,
    pub content_index: usize,
}

#[derive(Debug)]
pub enum FrontendEvent {
    TurnStarted,
    SteerQueued {
        queue_id: u64,
        text: String,
    },
    UserMessage {
        text: String,
    },
    AssistantDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    ToolCallPreparing {
        key: ToolDraftKey,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
    },
    ToolCallArguments {
        key: ToolDraftKey,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
        delta: String,
    },
    ToolCallPrepared {
        key: ToolDraftKey,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    TurnFinished,
    TurnFailed {
        message: String,
    },
    TurnCancelled,
    ToolCallStarted {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolCallUpdated {
        tool_call_id: String,
        update: ToolUpdatePresentation,
    },
    ToolCallFinished {
        tool_call_id: String,
        is_error: bool,
    },
    ToolApprovalRequested {
        approval_id: u64,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolApprovalResolved {
        approval_id: u64,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
    },
    ConfigChanged {
        provider: String,
        model_id: String,
        reasoning_effort: ReasoningLevel,
    },
}

#[derive(Debug, Clone)]
pub enum ToolUpdatePresentation {
    Text(String),
    Progress {
        label: String,
        current: Option<u64>,
        total: Option<u64>,
    },
    Details(serde_json::Value),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubmitTurnResult {
    Sent,
    NoBackendAttached,
}

#[derive(Clone)]
pub(crate) struct BackendCommands {
    command_tx: Option<CoreCommandSender>,
}

impl BackendCommands {
    pub(crate) fn new(command_tx: Option<CoreCommandSender>) -> Self {
        Self { command_tx }
    }

    pub(crate) fn submit_turn(&self, text: String) -> Result<SubmitTurnResult> {
        let Some(tx) = &self.command_tx else {
            return Ok(SubmitTurnResult::NoBackendAttached);
        };
        tx.try_send(CoreCommand::SubmitTurn { text })?;
        Ok(SubmitTurnResult::Sent)
    }

    pub(crate) fn cancel_active_turn(&self) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(CoreCommand::CancelActiveTurn)?;
        }
        Ok(())
    }

    pub(crate) fn cycle_reasoning_effort(&self) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(CoreCommand::CycleReasoningEffort)?;
        }
        Ok(())
    }

    pub(crate) fn approve(&self, approval_id: u64) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(CoreCommand::ApproveToolCall { approval_id })?;
        }
        Ok(())
    }

    pub(crate) fn reject(&self, approval_id: u64, reason: Option<String>) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.try_send(CoreCommand::RejectToolCall {
                approval_id,
                reason,
            })?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct CoreEventMapper {
    message_roles: HashMap<u64, MessageRole>,
}

impl CoreEventMapper {
    pub(crate) fn map(&mut self, event: CoreEvent) -> Option<FrontendEvent> {
        match event {
            CoreEvent::AgentStarted | CoreEvent::AgentFinished => None,
            CoreEvent::TurnStarted { .. } => Some(FrontendEvent::TurnStarted),
            CoreEvent::SteerQueued { queue_id, text } => Some(FrontendEvent::SteerQueued { queue_id, text }),
            CoreEvent::MessageStarted {
                message_id, role, ..
            } => {
                self.message_roles.insert(message_id, role);
                None
            }
            CoreEvent::MessageDelta {
                message_id,
                delta: MessageDelta::Text(text),
                ..
            } => match self.message_roles.get(&message_id).copied() {
                Some(MessageRole::User) => Some(FrontendEvent::UserMessage { text }),
                Some(MessageRole::Assistant) => Some(FrontendEvent::AssistantDelta { text }),
                Some(MessageRole::ToolResult) => None,
                Some(MessageRole::Status) | None => None,
            },
            CoreEvent::MessageDelta {
                message_id,
                delta: MessageDelta::Thinking(text),
                ..
            } => matches!(
                self.message_roles.get(&message_id),
                Some(MessageRole::Assistant)
            )
            .then_some(FrontendEvent::ThinkingDelta { text }),
            CoreEvent::MessageDelta {
                message_id,
                delta:
                    MessageDelta::ToolCall(ToolCallDelta::Start {
                        content_index,
                        tool_call_id,
                        tool_name,
                    }),
                ..
            } => matches!(
                self.message_roles.get(&message_id),
                Some(MessageRole::Assistant)
            )
            .then_some(FrontendEvent::ToolCallPreparing {
                key: ToolDraftKey {
                    message_id,
                    content_index,
                },
                tool_call_id,
                tool_name,
            }),
            CoreEvent::MessageDelta {
                message_id,
                delta:
                    MessageDelta::ToolCall(ToolCallDelta::Arguments {
                        content_index,
                        tool_call_id,
                        tool_name,
                        delta,
                    }),
                ..
            } => matches!(
                self.message_roles.get(&message_id),
                Some(MessageRole::Assistant)
            )
            .then_some(FrontendEvent::ToolCallArguments {
                key: ToolDraftKey {
                    message_id,
                    content_index,
                },
                tool_call_id,
                tool_name,
                delta,
            }),
            CoreEvent::MessageDelta {
                message_id,
                delta:
                    MessageDelta::ToolCall(ToolCallDelta::End {
                        content_index,
                        tool_call_id,
                        tool_name,
                        arguments,
                    }),
                ..
            } => matches!(
                self.message_roles.get(&message_id),
                Some(MessageRole::Assistant)
            )
            .then_some(FrontendEvent::ToolCallPrepared {
                key: ToolDraftKey {
                    message_id,
                    content_index,
                },
                tool_call_id,
                tool_name,
                arguments,
            }),
            CoreEvent::MessageFinished { message_id, .. } => {
                self.message_roles.remove(&message_id);
                None
            }
            CoreEvent::ToolCallStarted {
                tool_call_id,
                tool_name,
                arguments,
                ..
            } => Some(FrontendEvent::ToolCallStarted {
                tool_call_id,
                tool_name,
                arguments,
            }),
            CoreEvent::ToolCallUpdated {
                tool_call_id,
                update,
                ..
            } => Some(FrontendEvent::ToolCallUpdated {
                tool_call_id,
                update: lower_tool_update(update),
            }),
            CoreEvent::ToolCallFinished {
                tool_call_id,
                is_error,
                ..
            } => Some(FrontendEvent::ToolCallFinished {
                tool_call_id,
                is_error,
            }),
            CoreEvent::ToolApprovalRequested {
                approval_id,
                tool_call_id,
                tool_name,
                arguments,
                ..
            } => Some(FrontendEvent::ToolApprovalRequested {
                approval_id,
                tool_call_id,
                tool_name,
                arguments,
            }),
            CoreEvent::ToolApprovalResolved {
                approval_id,
                tool_call_id,
                approved,
                reason,
                ..
            } => Some(FrontendEvent::ToolApprovalResolved {
                approval_id,
                tool_call_id,
                approved,
                reason,
            }),
            CoreEvent::ToolResultStarted { .. } => None,
            CoreEvent::ToolResultFinished {
                tool_call_id,
                tool_name,
                text,
                details,
                is_error,
                ..
            } => Some(FrontendEvent::ToolResult {
                tool_call_id,
                tool_name,
                text,
                details,
                is_error,
            }),
            CoreEvent::TurnFinished { .. } => Some(FrontendEvent::TurnFinished),
            CoreEvent::TurnFailed { message, .. } => Some(FrontendEvent::TurnFailed { message }),
            CoreEvent::TurnCancelled { .. } => {
                self.message_roles.clear();
                Some(FrontendEvent::TurnCancelled)
            }
            CoreEvent::ConfigChanged {
                provider,
                model_id,
                reasoning_effort,
            } => Some(FrontendEvent::ConfigChanged {
                provider,
                model_id,
                reasoning_effort,
            }),
        }
    }
}

fn lower_tool_update(update: ToolUpdateEvent) -> ToolUpdatePresentation {
    match update {
        ToolUpdateEvent::Text(text) => ToolUpdatePresentation::Text(text),
        ToolUpdateEvent::Progress {
            label,
            current,
            total,
        } => ToolUpdatePresentation::Progress {
            label,
            current,
            total,
        },
        ToolUpdateEvent::Details(details) => ToolUpdatePresentation::Details(details),
    }
}

fn coalesce_frontend_events(events: impl IntoIterator<Item = FrontendEvent>) -> Vec<FrontendEvent> {
    let mut coalesced = Vec::new();
    for event in events {
        match event {
            FrontendEvent::AssistantDelta { text } => {
                if let Some(FrontendEvent::AssistantDelta { text: previous }) = coalesced.last_mut()
                {
                    previous.push_str(&text);
                    continue;
                }
                coalesced.push(FrontendEvent::AssistantDelta { text });
            }
            FrontendEvent::ThinkingDelta { text } => {
                if let Some(FrontendEvent::ThinkingDelta { text: previous }) = coalesced.last_mut()
                {
                    previous.push_str(&text);
                    continue;
                }
                coalesced.push(FrontendEvent::ThinkingDelta { text });
            }
            FrontendEvent::ToolCallArguments {
                key,
                tool_call_id,
                tool_name,
                delta,
            } => {
                if let Some(FrontendEvent::ToolCallArguments {
                    key: previous_key,
                    tool_call_id: previous_id,
                    tool_name: previous_name,
                    delta: previous,
                }) = coalesced.last_mut()
                    && previous_key == &key
                {
                    previous.push_str(&delta);
                    if previous_id.is_none() {
                        *previous_id = tool_call_id;
                    }
                    if previous_name.is_none() {
                        *previous_name = tool_name;
                    }
                    continue;
                }
                coalesced.push(FrontendEvent::ToolCallArguments {
                    key,
                    tool_call_id,
                    tool_name,
                    delta,
                });
            }
            FrontendEvent::ToolCallUpdated {
                tool_call_id,
                update,
            } => {
                if let Some(FrontendEvent::ToolCallUpdated {
                    tool_call_id: previous_id,
                    update: previous,
                }) = coalesced.last_mut()
                    && previous_id == &tool_call_id
                {
                    let same_snapshot_kind = matches!(
                        (&*previous, &update),
                        (
                            ToolUpdatePresentation::Progress { .. },
                            ToolUpdatePresentation::Progress { .. }
                        ) | (
                            ToolUpdatePresentation::Details(_),
                            ToolUpdatePresentation::Details(_)
                        )
                    );
                    if same_snapshot_kind {
                        *previous = update;
                        continue;
                    }
                }
                coalesced.push(FrontendEvent::ToolCallUpdated {
                    tool_call_id,
                    update,
                });
            }
            event => coalesced.push(event),
        }
    }
    coalesced
}

pub(crate) struct CoreBridge {
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl fmt::Debug for CoreBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoreBridge").finish_non_exhaustive()
    }
}

impl CoreBridge {
    pub(crate) fn spawn(mut receiver: CoreEventReceiver, handle: AppHandle<IyonAction>) -> Self {
        let cancel = CancellationToken::new();
        let child = cancel.child_token();
        let task = tokio::spawn(async move {
            let mut mapper = CoreEventMapper::default();
            loop {
                let first = tokio::select! {
                    _ = child.cancelled() => break,
                    event = receiver.recv_event() => event,
                };
                let Some(first) = first else {
                    break;
                };
                let mut events = vec![first];
                events.extend(receiver.drain_events());
                let frontend = coalesce_frontend_events(
                    events.into_iter().filter_map(|event| mapper.map(event)),
                );
                for event in frontend {
                    let action = IyonAction::Backend(event);
                    let result = tokio::select! {
                        _ = child.cancelled() => return,
                        result = handle.send_async(action) => result,
                    };
                    if result.is_err() {
                        return;
                    }
                }
                tokio::task::yield_now().await;
            }
        });
        Self {
            cancel,
            task: Some(task),
        }
    }

    pub(crate) async fn shutdown(&mut self) -> Result<()> {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.await.map_err(|error| anyhow::anyhow!(error))?;
        }
        Ok(())
    }
}

impl Drop for CoreBridge {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapper_preserves_message_role_and_thinking_order() {
        let mut mapper = CoreEventMapper::default();
        assert!(
            mapper
                .map(CoreEvent::MessageStarted {
                    turn_id: 1,
                    message_id: 2,
                    role: MessageRole::Assistant,
                })
                .is_none()
        );
        assert!(matches!(
            mapper.map(CoreEvent::MessageDelta {
                turn_id: 1,
                message_id: 2,
                delta: MessageDelta::Thinking("think".into()),
            }),
            Some(FrontendEvent::ThinkingDelta { text }) if text == "think"
        ));
        assert!(matches!(
            mapper.map(CoreEvent::MessageDelta {
                turn_id: 1,
                message_id: 2,
                delta: MessageDelta::Text("answer".into()),
            }),
            Some(FrontendEvent::AssistantDelta { text }) if text == "answer"
        ));
    }

    #[test]
    fn mapper_maps_tool_call_construction_before_execution() {
        let mut mapper = CoreEventMapper::default();
        assert!(
            mapper
                .map(CoreEvent::MessageStarted {
                    turn_id: 1,
                    message_id: 2,
                    role: MessageRole::Assistant,
                })
                .is_none()
        );

        let preparing = mapper
            .map(CoreEvent::MessageDelta {
                turn_id: 1,
                message_id: 2,
                delta: MessageDelta::ToolCall(ToolCallDelta::Start {
                    content_index: 3,
                    tool_call_id: Some("draft-id".into()),
                    tool_name: Some("search".into()),
                }),
            })
            .expect("mapped tool call preparation");
        assert!(matches!(
            preparing,
            FrontendEvent::ToolCallPreparing {
                key: ToolDraftKey {
                    message_id: 2,
                    content_index: 3,
                },
                ..
            }
        ));

        let arguments = mapper
            .map(CoreEvent::MessageDelta {
                turn_id: 1,
                message_id: 2,
                delta: MessageDelta::ToolCall(ToolCallDelta::Arguments {
                    content_index: 3,
                    tool_call_id: None,
                    tool_name: None,
                    delta: "{\"q\":".into(),
                }),
            })
            .expect("mapped tool call arguments");
        assert!(matches!(
            arguments,
            FrontendEvent::ToolCallArguments {
                key: ToolDraftKey {
                    message_id: 2,
                    content_index: 3,
                },
                delta,
                ..
            } if delta == "{\"q\":"
        ));

        let prepared = mapper
            .map(CoreEvent::MessageDelta {
                turn_id: 1,
                message_id: 2,
                delta: MessageDelta::ToolCall(ToolCallDelta::End {
                    content_index: 3,
                    tool_call_id: "authoritative-id".into(),
                    tool_name: "search".into(),
                    arguments: serde_json::json!({"q": "iyon"}),
                }),
            })
            .expect("mapped prepared tool call");
        assert!(matches!(
            prepared,
            FrontendEvent::ToolCallPrepared {
                key: ToolDraftKey {
                    message_id: 2,
                    content_index: 3,
                },
                tool_call_id,
                ..
            } if tool_call_id == "authoritative-id"
        ));

        assert!(matches!(
            mapper.map(CoreEvent::ToolCallStarted {
                turn_id: 1,
                message_id: 2,
                tool_call_id: "authoritative-id".into(),
                tool_name: "search".into(),
                arguments: serde_json::json!({"q": "iyon"}),
            }),
            Some(FrontendEvent::ToolCallStarted { .. })
        ));
    }

    #[test]
    fn adjacent_tool_call_arguments_coalesce_for_the_same_draft() {
        let key = ToolDraftKey {
            message_id: 7,
            content_index: 1,
        };
        let events = coalesce_frontend_events([
            FrontendEvent::ToolCallArguments {
                key,
                tool_call_id: Some("call".into()),
                tool_name: Some("search".into()),
                delta: "{\"q\":".into(),
            },
            FrontendEvent::ToolCallArguments {
                key,
                tool_call_id: None,
                tool_name: None,
                delta: "\"iyon\"}".into(),
            },
        ]);

        assert!(matches!(
            &events[..],
            [FrontendEvent::ToolCallArguments { delta, .. }] if delta == "{\"q\":\"iyon\"}"
        ));
    }

    #[test]
    fn tool_call_arguments_do_not_coalesce_across_content_indexes() {
        let events = coalesce_frontend_events([
            FrontendEvent::ToolCallArguments {
                key: ToolDraftKey {
                    message_id: 7,
                    content_index: 0,
                },
                tool_call_id: None,
                tool_name: None,
                delta: "first".into(),
            },
            FrontendEvent::ToolCallArguments {
                key: ToolDraftKey {
                    message_id: 7,
                    content_index: 1,
                },
                tool_call_id: None,
                tool_name: None,
                delta: "second".into(),
            },
        ]);

        assert!(matches!(
            &events[..],
            [
                FrontendEvent::ToolCallArguments { delta: first, .. },
                FrontendEvent::ToolCallArguments { delta: second, .. },
            ] if first == "first" && second == "second"
        ));
    }

    #[test]
    fn tool_call_preparing_and_prepared_are_not_discarded_by_coalescing() {
        let key = ToolDraftKey {
            message_id: 7,
            content_index: 1,
        };
        let events = coalesce_frontend_events([
            FrontendEvent::ToolCallPreparing {
                key,
                tool_call_id: None,
                tool_name: Some("search".into()),
            },
            FrontendEvent::ToolCallArguments {
                key,
                tool_call_id: None,
                tool_name: None,
                delta: "{}".into(),
            },
            FrontendEvent::ToolCallPrepared {
                key,
                tool_call_id: "call".into(),
                tool_name: "search".into(),
                arguments: serde_json::json!({}),
            },
        ]);

        assert!(matches!(
            &events[..],
            [
                FrontendEvent::ToolCallPreparing { .. },
                FrontendEvent::ToolCallArguments { .. },
                FrontendEvent::ToolCallPrepared { tool_call_id, .. },
            ] if tool_call_id == "call"
        ));
    }

    #[test]
    fn mapper_cancellation_clears_transient_message_state() {
        let mut mapper = CoreEventMapper::default();
        assert!(
            mapper
                .map(CoreEvent::MessageStarted {
                    turn_id: 1,
                    message_id: 2,
                    role: MessageRole::Assistant,
                })
                .is_none()
        );
        assert!(
            mapper
                .map(CoreEvent::MessageDelta {
                    turn_id: 1,
                    message_id: 2,
                    delta: MessageDelta::ToolCall(ToolCallDelta::Start {
                        content_index: 0,
                        tool_call_id: None,
                        tool_name: None,
                    }),
                })
                .is_some()
        );

        assert!(matches!(
            mapper.map(CoreEvent::TurnCancelled { turn_id: 1 }),
            Some(FrontendEvent::TurnCancelled)
        ));
        assert!(
            mapper
                .map(CoreEvent::MessageDelta {
                    turn_id: 1,
                    message_id: 2,
                    delta: MessageDelta::ToolCall(ToolCallDelta::Arguments {
                        content_index: 0,
                        tool_call_id: None,
                        tool_name: None,
                        delta: "{}".into(),
                    }),
                })
                .is_none()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn bridge_delivers_core_stream_events_to_the_headless_app() {
        let core = iyon_core::IyonCore::spawn_default_on_current_runtime();
        let (commands, receiver) = core.split();
        let selection = iyon_core::ModelSelection {
            provider: "mock".into(),
            model_id: "mock".into(),
        };
        let app = super::super::build_app(commands.clone(), selection);
        let mut harness = iyon_tui::testing::start(app, 80, 20).unwrap();
        let mut bridge = CoreBridge::spawn(receiver, harness.handle());
        commands
            .try_send(iyon_core::CoreCommand::SubmitTurn {
                text: "hello".into(),
            })
            .unwrap();

        let mut saw_stream = false;
        for _ in 0..80 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            harness
                .advance_time(std::time::Duration::from_millis(20))
                .unwrap();
            if harness
                .screen_lines()
                .iter()
                .any(|line| line.contains("Mock response"))
            {
                saw_stream = true;
                break;
            }
        }
        bridge.shutdown().await.unwrap();
        assert!(saw_stream, "core stream never reached the application");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn bridge_shutdown_cancels_an_idle_receiver() {
        let core = iyon_core::IyonCore::spawn_default_on_current_runtime();
        let (commands, receiver) = core.split();
        let app = super::super::build_app(
            commands,
            iyon_core::ModelSelection {
                provider: "mock".into(),
                model_id: "mock".into(),
            },
        );
        let handle = app.handle();
        let mut bridge = CoreBridge::spawn(receiver, handle);
        bridge.shutdown().await.unwrap();
    }

    #[test]
    fn bridge_coalesces_only_safe_adjacent_events() {
        let events = coalesce_frontend_events([
            FrontendEvent::AssistantDelta { text: "a".into() },
            FrontendEvent::AssistantDelta { text: "b".into() },
            FrontendEvent::ThinkingDelta { text: "t".into() },
            FrontendEvent::ThinkingDelta { text: "h".into() },
            FrontendEvent::TurnFinished,
            FrontendEvent::ToolCallUpdated {
                tool_call_id: "tool".into(),
                update: ToolUpdatePresentation::Progress {
                    label: "one".into(),
                    current: Some(1),
                    total: Some(3),
                },
            },
            FrontendEvent::ToolCallUpdated {
                tool_call_id: "tool".into(),
                update: ToolUpdatePresentation::Progress {
                    label: "two".into(),
                    current: Some(2),
                    total: Some(3),
                },
            },
        ]);
        assert!(matches!(&events[0], FrontendEvent::AssistantDelta { text } if text == "ab"));
        assert!(matches!(&events[1], FrontendEvent::ThinkingDelta { text } if text == "th"));
        assert!(matches!(&events[2], FrontendEvent::TurnFinished));
        assert!(matches!(
            &events[3],
            FrontendEvent::ToolCallUpdated {
                update: ToolUpdatePresentation::Progress {
                    current: Some(2),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn mapper_keeps_tool_result_details_from_finished_event() {
        let mut mapper = CoreEventMapper::default();
        let details = serde_json::json!({"diff": "@@ -1 +1 @@\n-old\n+new\n"});
        assert!(
            mapper
                .map(CoreEvent::ToolResultStarted {
                    turn_id: 1,
                    message_id: 8,
                    tool_call_id: "tool".into(),
                    tool_name: "edit".into(),
                    is_error: false,
                })
                .is_none()
        );
        assert!(
            mapper
                .map(CoreEvent::MessageStarted {
                    turn_id: 1,
                    message_id: 8,
                    role: MessageRole::ToolResult,
                })
                .is_none()
        );
        assert!(
            mapper
                .map(CoreEvent::MessageDelta {
                    turn_id: 1,
                    message_id: 8,
                    delta: MessageDelta::Text("Successfully replaced 1 block(s).".into()),
                })
                .is_none()
        );
        assert!(
            mapper
                .map(CoreEvent::MessageFinished {
                    turn_id: 1,
                    message_id: 8,
                })
                .is_none(),
            "message finish must not publish a details-less result"
        );
        let event = mapper
            .map(CoreEvent::ToolResultFinished {
                turn_id: 1,
                message_id: 8,
                tool_call_id: "tool".into(),
                tool_name: "edit".into(),
                text: "Successfully replaced 1 block(s).".into(),
                details: details.clone(),
                is_error: false,
            })
            .expect("mapped result");
        match event {
            FrontendEvent::ToolResult {
                text,
                details: mapped,
                is_error,
                ..
            } => {
                assert_eq!(text, "Successfully replaced 1 block(s).");
                assert_eq!(mapped, details);
                assert!(!is_error);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
