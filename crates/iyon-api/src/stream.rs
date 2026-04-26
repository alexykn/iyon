use serde_json::Value;

#[derive(Debug, Clone)]
pub enum ModelStreamEvent {
    Started,

    TextStart {
        content_index: usize,
    },
    TextDelta {
        content_index: usize,
        delta: String,
    },
    TextEnd {
        content_index: usize,
        text: String,
    },

    ThinkingStart {
        content_index: usize,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
    },
    ThinkingEnd {
        content_index: usize,
        text: String,
    },

    ToolCallStart {
        content_index: usize,
        id: Option<String>,
        name: Option<String>,
    },
    ToolCallDelta {
        content_index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    ToolCallEnd {
        content_index: usize,
        id: String,
        name: String,
        arguments: Value,
    },

    Usage {
        usage: Usage,
    },
    Done {
        stop_reason: StopReason,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}
