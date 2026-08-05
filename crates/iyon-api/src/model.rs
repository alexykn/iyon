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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningLevel {
    None,
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    XHigh,
    Max,
}

impl ReasoningLevel {
    /// The full OpenAI-style effort set in ascending order. Both OpenAI and
    /// OpenRouter use these exact effort names.
    pub const ALL: [ReasoningLevel; 7] = [
        ReasoningLevel::None,
        ReasoningLevel::Minimal,
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
        ReasoningLevel::XHigh,
        ReasoningLevel::Max,
    ];

    /// The wire-level effort string sent to providers.
    pub fn code(self) -> &'static str {
        match self {
            ReasoningLevel::None => "none",
            ReasoningLevel::Minimal => "minimal",
            ReasoningLevel::Low => "low",
            ReasoningLevel::Medium => "medium",
            ReasoningLevel::High => "high",
            ReasoningLevel::XHigh => "xhigh",
            ReasoningLevel::Max => "max",
        }
    }

    /// Candidate effort levels a provider accepts, in ascending cycle order.
    ///
    /// Both OpenAI and OpenRouter accept the full set (model-dependent for
    /// OpenAI; OpenRouter narrows per model via `/api/v1/models`
    /// `reasoning.supported_efforts`, which we will consult once the model
    /// catalog lands). Non-reasoning providers (e.g. mock) return none.
    pub fn candidates(provider: &str) -> &'static [ReasoningLevel] {
        match provider {
            "openai-codex" | "openai" | "codex" | "openrouter" => &ReasoningLevel::ALL,
            _ => &[],
        }
    }

    /// The next effort level after `current` in cycle order for `provider`,
    /// wrapping around. Returns `current` unchanged when the provider has no
    /// reasoning candidates (e.g. mock).
    ///
    /// Shared by the core (authoritative selection) and the TUI (optimistic
    /// draw-ahead) so both always compute the exact same step from the same
    /// base value, which keeps them in lockstep.
    pub fn next_for(current: ReasoningLevel, provider: &str) -> ReasoningLevel {
        let candidates = Self::candidates(provider);
        if candidates.is_empty() {
            return current;
        }
        let index = candidates
            .iter()
            .position(|&level| level == current)
            .unwrap_or(0);
        candidates[(index + 1) % candidates.len()]
    }
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
