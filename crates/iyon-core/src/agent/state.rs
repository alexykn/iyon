#![allow(dead_code)]

use std::collections::HashSet;

use crate::{
    agent::transcript::AgentMessage,
    ids::{MessageId, ToolCallId, TurnId},
};

#[derive(Debug, Default)]
pub struct AgentState {
    pub is_running: bool,
    pub active_turn_id: Option<TurnId>,
    pub active_message_id: Option<MessageId>,
    pub pending_tool_calls: HashSet<ToolCallId>,
    pub queued_steering: Vec<AgentMessage>,
    pub queued_followups: Vec<AgentMessage>,
    pub error_message: Option<String>,
}
