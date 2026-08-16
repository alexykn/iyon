use iyon_api::{ModelMetadata, ModelParams, ModelRequest};

use crate::{kernel::KernelSession, session::state::SessionState, tools::ToolRegistry};

pub(crate) struct RequestBuildContext<'a> {
    pub session: &'a SessionState,
    pub tools: Option<&'a ToolRegistry>,
}

pub(crate) fn build_model_request(ctx: RequestBuildContext<'_>) -> ModelRequest {
    let mut params = ModelParams::default();
    params.reasoning = Some(ctx.session.reasoning_effort);

    ModelRequest {
        system_prompt: non_empty_system_prompt(&ctx.session.system_prompt),
        messages: KernelSession::from_legacy(ctx.session).to_model_messages(),
        tools: ctx.tools.map_or_else(Vec::new, ToolRegistry::model_specs),
        params,
        metadata: lower_metadata(ctx.session),
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
