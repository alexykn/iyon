use iyon_api::ReasoningLevel;

#[derive(Debug, Clone)]
pub enum CoreEvent {
    AgentStarted,
    AgentFinished,
    TurnStarted {
        turn_id: u64,
    },
    /// A message was queued for steering (delivered to the agent at the next turn
    /// boundary). Lets the UI show it as pending until it is actually sent.
    SteerQueued {
        text: String,
    },
    MessageStarted {
        turn_id: u64,
        message_id: u64,
        role: MessageRole,
    },
    MessageDelta {
        turn_id: u64,
        message_id: u64,
        delta: MessageDelta,
    },
    MessageFinished {
        turn_id: u64,
        message_id: u64,
    },
    ToolCallStarted {
        turn_id: u64,
        message_id: u64,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolCallFinished {
        turn_id: u64,
        message_id: u64,
        tool_call_id: String,
        tool_name: String,
        is_error: bool,
    },
    ToolCallUpdated {
        turn_id: u64,
        message_id: u64,
        tool_call_id: String,
        tool_name: String,
        update: ToolUpdateEvent,
    },
    ToolResultStarted {
        turn_id: u64,
        message_id: u64,
        tool_call_id: String,
        tool_name: String,
        is_error: bool,
    },
    ToolResultFinished {
        turn_id: u64,
        message_id: u64,
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
    },
    ToolApprovalRequested {
        turn_id: u64,
        approval_id: u64,
        message_id: u64,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolApprovalResolved {
        turn_id: u64,
        approval_id: u64,
        tool_call_id: String,
        approved: bool,
        reason: Option<String>,
    },
    TurnFinished {
        turn_id: u64,
    },
    TurnFailed {
        turn_id: u64,
        message: String,
    },
    TurnCancelled {
        turn_id: u64,
    },
    /// Emitted at core start and whenever the current provider/model or reasoning
    /// effort changes. Source of truth for the TUI status bar.
    ConfigChanged {
        provider: String,
        model_id: String,
        reasoning_effort: ReasoningLevel,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    ToolResult,
    Status,
}

#[derive(Debug, Clone)]
pub enum ToolUpdateEvent {
    Text(String),
    Progress {
        label: String,
        current: Option<u64>,
        total: Option<u64>,
    },
    Details(serde_json::Value),
}

#[derive(Debug, Clone)]
pub enum MessageDelta {
    Text(String),
    /// Streamed reasoning/thinking text (muted + italic in the TUI).
    Thinking(String),
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        arguments_delta: Option<String>,
    },
}
