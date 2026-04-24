#[derive(Debug, Clone, Default)]
pub struct ModelRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolSpec>,
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
    Thinking {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Clone)]
pub struct ModelToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: String,
}
