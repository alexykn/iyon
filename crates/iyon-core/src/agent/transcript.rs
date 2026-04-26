#![allow(dead_code)]

use std::time::SystemTime;

use iyon_api::{ContentBlock, StopReason, Usage};
use serde_json::Value;

use crate::ids::{MessageId, ToolCallId};

#[derive(Debug, Clone)]
pub enum AgentMessage {
    User {
        id: MessageId,
        content: Vec<ContentBlock>,
        timestamp: SystemTime,
    },
    Assistant {
        id: MessageId,
        content: Vec<ContentBlock>,
        usage: Option<Usage>,
        stop_reason: Option<StopReason>,
        timestamp: SystemTime,
    },
    ToolResult {
        id: MessageId,
        tool_call_id: ToolCallId,
        tool_name: String,
        content: Vec<ContentBlock>,
        details: Value,
        is_error: bool,
        timestamp: SystemTime,
    },
    Status {
        id: MessageId,
        text: String,
        timestamp: SystemTime,
    },
}

impl AgentMessage {
    pub fn id(&self) -> MessageId {
        match self {
            AgentMessage::User { id, .. }
            | AgentMessage::Assistant { id, .. }
            | AgentMessage::ToolResult { id, .. }
            | AgentMessage::Status { id, .. } => *id,
        }
    }
}
