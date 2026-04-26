use iyon_api::{ModelMessage, ModelMetadata, ModelRequest};

use crate::{agent::transcript::AgentMessage, session::state::SessionState, tools::ToolRegistry};

pub(crate) struct RequestBuildContext<'a> {
    pub session: &'a SessionState,
    pub tools: Option<&'a ToolRegistry>,
}

pub(crate) fn build_model_request(ctx: RequestBuildContext<'_>) -> ModelRequest {
    ModelRequest {
        system_prompt: non_empty_system_prompt(&ctx.session.system_prompt),
        messages: lower_messages(&ctx.session.messages),
        tools: ctx.tools.map_or_else(Vec::new, ToolRegistry::model_specs),
        params: Default::default(),
        metadata: lower_metadata(ctx.session),
    }
}

fn lower_messages(messages: &[AgentMessage]) -> Vec<ModelMessage> {
    messages.iter().filter_map(lower_message).collect()
}

fn lower_message(message: &AgentMessage) -> Option<ModelMessage> {
    match message {
        AgentMessage::User { content, .. } => Some(ModelMessage::User {
            content: content.clone(),
        }),
        AgentMessage::Assistant { content, .. } => Some(ModelMessage::Assistant {
            content: content.clone(),
        }),
        AgentMessage::ToolResult {
            tool_call_id,
            tool_name,
            content,
            is_error,
            ..
        } => Some(ModelMessage::ToolResult {
            tool_call_id: tool_call_id.0.clone(),
            tool_name: tool_name.clone(),
            content: content.clone(),
            is_error: *is_error,
        }),
        AgentMessage::Status { .. } => None,
    }
}

fn lower_metadata(session: &SessionState) -> ModelMetadata {
    ModelMetadata {
        session_id: Some(session.id.0.to_string()),
        user_id: session.metadata.user_id.clone(),
    }
}

fn non_empty_system_prompt(prompt: &str) -> Option<String> {
    let prompt = prompt.trim();
    (!prompt.is_empty()).then(|| prompt.to_string())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::SystemTime};

    use iyon_api::{ContentBlock, ModelMessage};
    use serde_json::json;

    use super::*;
    use crate::{
        agent::transcript::AgentMessage,
        ids::{MessageId, SessionId, ToolCallId},
        session::state::SessionState,
        tools::ToolRegistry,
    };

    #[test]
    fn preserves_user_assistant_order() {
        let mut session = SessionState::new(SessionId(7), PathBuf::from("/tmp"));
        session.messages = vec![user(1, "hello"), assistant(2, "hi"), user(3, "next")];

        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: None,
        });

        assert_eq!(request.messages.len(), 3);
        assert_text_message(&request.messages[0], "user", "hello");
        assert_text_message(&request.messages[1], "assistant", "hi");
        assert_text_message(&request.messages[2], "user", "next");
    }

    #[test]
    fn filters_status_messages() {
        let mut session = SessionState::new(SessionId(7), PathBuf::from("/tmp"));
        session.messages = vec![
            user(1, "hello"),
            AgentMessage::Status {
                id: MessageId(2),
                text: "working".to_string(),
                timestamp: SystemTime::now(),
            },
            assistant(3, "hi"),
        ];

        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: None,
        });

        assert_eq!(request.messages.len(), 2);
        assert_text_message(&request.messages[0], "user", "hello");
        assert_text_message(&request.messages[1], "assistant", "hi");
    }

    #[test]
    fn lowers_tool_result_without_details() {
        let mut session = SessionState::new(SessionId(7), PathBuf::from("/tmp"));
        session.messages = vec![AgentMessage::ToolResult {
            id: MessageId(1),
            tool_call_id: ToolCallId("call-1".to_string()),
            tool_name: "read".to_string(),
            content: vec![ContentBlock::Text {
                text: "contents".to_string(),
            }],
            details: json!({ "path": "file.txt" }),
            is_error: false,
            timestamp: SystemTime::now(),
        }];

        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: None,
        });

        assert_eq!(request.messages.len(), 1);
        let ModelMessage::ToolResult {
            tool_call_id,
            tool_name,
            content,
            is_error,
        } = &request.messages[0]
        else {
            panic!("expected tool result message");
        };
        assert_eq!(tool_call_id, "call-1");
        assert_eq!(tool_name, "read");
        assert!(!is_error);
        assert!(matches!(
            &content[..],
            [ContentBlock::Text { text }] if text == "contents"
        ));
    }

    #[test]
    fn lowers_metadata_and_system_prompt() {
        let mut session = SessionState::new(SessionId(42), PathBuf::from("/tmp"));
        session.system_prompt = "system".to_string();
        session.metadata.user_id = Some("user".to_string());

        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: None,
        });

        assert_eq!(request.system_prompt.as_deref(), Some("system"));
        assert_eq!(request.metadata.session_id.as_deref(), Some("42"));
        assert_eq!(request.metadata.user_id.as_deref(), Some("user"));

        session.system_prompt.clear();
        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: None,
        });
        assert_eq!(request.system_prompt, None);
    }

    #[test]
    fn preserves_assistant_tool_calls() {
        let mut session = SessionState::new(SessionId(7), PathBuf::from("/tmp"));
        session.messages = vec![AgentMessage::Assistant {
            id: MessageId(1),
            content: vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                arguments: json!({ "path": "file.txt" }),
            }],
            usage: None,
            stop_reason: None,
            timestamp: SystemTime::now(),
        }];

        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: None,
        });

        assert!(matches!(
            &request.messages[..],
            [ModelMessage::Assistant { content }]
                if matches!(&content[..], [ContentBlock::ToolCall { id, name, arguments }]
                    if id == "call-1" && name == "read" && arguments["path"] == "file.txt")
        ));
    }

    #[test]
    fn includes_active_tool_specs_when_registry_is_provided() {
        let session = SessionState::new(SessionId(7), PathBuf::from("/tmp"));
        let mut tools = ToolRegistry::new();
        tools.register_builtin_defaults().unwrap();

        let request = build_model_request(RequestBuildContext {
            session: &session,
            tools: Some(&tools),
        });

        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0].name, "read");
        assert_eq!(request.tools[0].input_schema["type"], "object");
    }

    fn user(id: u64, text: &str) -> AgentMessage {
        AgentMessage::User {
            id: MessageId(id),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            timestamp: SystemTime::now(),
        }
    }

    fn assistant(id: u64, text: &str) -> AgentMessage {
        AgentMessage::Assistant {
            id: MessageId(id),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            usage: None,
            stop_reason: None,
            timestamp: SystemTime::now(),
        }
    }

    fn assert_text_message(message: &ModelMessage, role: &str, expected_text: &str) {
        let content = match (role, message) {
            ("user", ModelMessage::User { content }) => content,
            ("assistant", ModelMessage::Assistant { content }) => content,
            _ => panic!("unexpected message role"),
        };

        assert!(matches!(
            &content[..],
            [ContentBlock::Text { text }] if text == expected_text
        ));
    }
}
