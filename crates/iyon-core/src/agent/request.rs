use iyon_api::{ModelMessage, ModelMetadata, ModelParams, ModelRequest};

use crate::{agent::transcript::AgentMessage, session::state::SessionState, tools::ToolRegistry};

pub(crate) struct RequestBuildContext<'a> {
    pub session: &'a SessionState,
    pub tools: Option<&'a ToolRegistry>,
}

pub(crate) fn build_model_request(ctx: RequestBuildContext<'_>) -> ModelRequest {
    let mut params = ModelParams::default();
    params.reasoning = Some(ctx.session.reasoning_effort);

    ModelRequest {
        system_prompt: non_empty_system_prompt(&ctx.session.system_prompt),
        messages: lower_messages(&ctx.session.messages),
        tools: ctx.tools.map_or_else(Vec::new, ToolRegistry::model_specs),
        params,
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
