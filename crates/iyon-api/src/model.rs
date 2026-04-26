use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct ModelRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolSpec>,
    pub params: ModelParams,
    pub metadata: ModelMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct ModelParams {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub reasoning: Option<ReasoningLevel>,
    pub cache_retention: Option<CacheRetention>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelMetadata {
    pub session_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningLevel {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheRetention {
    None,
    Short,
    Long,
}

#[derive(Debug, Clone)]
pub enum ModelMessage {
    User {
        content: Vec<ContentBlock>,
    },
    Assistant {
        content: Vec<ContentBlock>,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: Vec<ContentBlock>,
        is_error: bool,
    },
}

#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        data: Vec<u8>,
        mime_type: String,
    },
    Thinking {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
}

#[derive(Debug, Clone)]
pub struct ModelToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
