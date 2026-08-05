use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_stream::try_stream;
use futures_util::StreamExt;
use reqwest::{
    StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde_json::{Value, json};

use crate::{
    ContentBlock, ModelApi, ModelError, ModelErrorKind, ModelMessage, ModelRequest, ModelStream,
    ModelStreamEvent, ModelStreamFuture, ModelToolSpec, StopReason, Usage,
};

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const MAX_RETRIES: u32 = 2;
const BASE_DELAY_MS: u64 = 300;

/// OpenAI-chat-compatible client over the OpenRouter gateway.
///
/// Uses the `/chat/completions` endpoint. The model id is fixed at construction
/// time (matching how the Codex provider bakes in its model); per-turn model
/// switching is a separate, later change.
#[derive(Clone)]
pub struct OpenRouterModelApi {
    client: reqwest::Client,
    api_key: Arc<str>,
    model_id: Arc<str>,
    base_url: Arc<str>,
}

impl OpenRouterModelApi {
    pub fn new(
        api_key: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<Self, ModelError> {
        Self::with_base_url(api_key, model_id, DEFAULT_BASE_URL)
    }

    pub fn with_base_url(
        api_key: impl Into<String>,
        model_id: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("accept", HeaderValue::from_static("text/event-stream"));
        // Optional app attribution so requests show up on openrouter.ai.
        if let Ok(title) = std::env::var("OPENROUTER_TITLE")
            && !title.trim().is_empty()
            && let Ok(value) = HeaderValue::from_str(&title)
        {
            headers.insert("x-openrouter-title", value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| ModelError::new(ModelErrorKind::Transport, e.to_string()))?;

        Ok(Self {
            client,
            api_key: api_key.into().into(),
            model_id: model_id.into().into(),
            base_url: base_url.into().into(),
        })
    }

    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    async fn send_with_retry(&self, body: &Value) -> Result<reqwest::Response, ModelError> {
        let url = self.completions_url();
        let bearer = format!("Bearer {}", self.api_key);
        let mut last_error: Option<ModelError> = None;

        for attempt in 0..=MAX_RETRIES {
            let result = self
                .client
                .post(&url)
                .header(AUTHORIZATION, &bearer)
                .json(body)
                .send()
                .await;

            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    let mapped = ModelError::new(
                        ModelErrorKind::Transport,
                        format!("openrouter transport error (attempt {}): {error}", attempt + 1),
                    );
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(backoff_duration(attempt)).await;
                        last_error = Some(mapped);
                        continue;
                    }
                    return Err(mapped);
                }
            };

            if response.status().is_success() {
                return Ok(response);
            }

            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            if should_retry_status(status) && attempt < MAX_RETRIES {
                tokio::time::sleep(backoff_duration(attempt)).await;
                last_error = Some(normalize_http_error(status, &text));
                continue;
            }
            return Err(normalize_http_error(status, &text));
        }

        Err(last_error.unwrap_or_else(|| ModelError::unknown("openrouter request failed")))
    }
}

impl ModelApi for OpenRouterModelApi {
    fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_> {
        Box::pin(async move {
            let body = build_request_body(request, &self.model_id);
            let response = self.send_with_retry(&body).await?;

            let stream = try_stream! {
                yield ModelStreamEvent::Started;

                let mut bytes = response.bytes_stream();
                let mut buffer = String::new();
                let mut tool_calls: BTreeMap<usize, ToolCallBuffer> = BTreeMap::new();
                let mut any_tool_call = false;
                let mut stop_reason = StopReason::Stop;

                while let Some(chunk) = bytes.next().await {
                    let chunk = chunk
                        .map_err(|e| ModelError::new(ModelErrorKind::Transport, e.to_string()))?;
                    buffer.push_str(&String::from_utf8_lossy(&chunk));

                    while let Some(idx) = buffer.find("\n\n") {
                        let frame = buffer[..idx].to_string();
                        buffer = buffer[idx + 2..].to_string();
                        let Some(data) = sse_data(&frame) else { continue; };
                        if data == "[DONE]" {
                            continue;
                        }

                        let data: Value = serde_json::from_str(&data)
                            .map_err(|e| ModelError::new(ModelErrorKind::Provider, format!("invalid chat chunk json: {e}")))?;

                        if let Some(message) = data.get("error").and_then(Value::as_str) {
                            Err::<(), _>(ModelError::new(ModelErrorKind::Provider, message))?;
                        }

                        let mut events: Vec<ModelStreamEvent> = Vec::new();

                        if let Some(choices) = data.get("choices").and_then(Value::as_array)
                            && let Some(choice) = choices.first()
                        {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(tool_calls_json) = delta.get("tool_calls").and_then(Value::as_array) {
                                    for tc in tool_calls_json {
                                        let index = tc.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                                        let entry = tool_calls.entry(index).or_default();
                                        if let Some(id) = tc.get("id").and_then(Value::as_str)
                                            && entry.id.is_empty()
                                        {
                                            entry.id = id.to_string();
                                        }
                                        if let Some(name) = tc
                                            .get("function")
                                            .and_then(|f| f.get("name"))
                                            .and_then(Value::as_str)
                                            && entry.name.is_empty()
                                        {
                                            entry.name = name.to_string();
                                        }
                                        if !entry.started && !entry.id.is_empty() && !entry.name.is_empty() {
                                            entry.started = true;
                                            any_tool_call = true;
                                            events.push(ModelStreamEvent::ToolCallStart {
                                                content_index: index,
                                                id: Some(entry.id.clone()),
                                                name: Some(entry.name.clone()),
                                            });
                                        }
                                        if let Some(args) = tc
                                            .get("function")
                                            .and_then(|f| f.get("arguments"))
                                            .and_then(Value::as_str)
                                            && !args.is_empty()
                                        {
                                            entry.arguments.push_str(args);
                                            events.push(ModelStreamEvent::ToolCallDelta {
                                                content_index: index,
                                                id: Some(entry.id.clone()),
                                                name: Some(entry.name.clone()),
                                                arguments_delta: args.to_string(),
                                            });
                                        }
                                    }
                                }

                                if let Some(reasoning) = delta.get("reasoning").and_then(Value::as_str)
                                    && !reasoning.is_empty()
                                {
                                    events.push(ModelStreamEvent::ThinkingDelta {
                                        content_index: 0,
                                        delta: reasoning.to_string(),
                                    });
                                }

                                if let Some(content) = delta.get("content").and_then(Value::as_str)
                                    && !content.is_empty()
                                {
                                    events.push(ModelStreamEvent::TextDelta {
                                        content_index: 0,
                                        delta: content.to_string(),
                                    });
                                }
                            }

                            if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str)
                                && !finish_reason.is_empty()
                            {
                                stop_reason = map_finish_reason(finish_reason);
                            }
                        }

                        if let Some(usage) = data.get("usage") {
                            let input = usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0);
                            let output = usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0);
                            let cached = usage
                                .get("prompt_tokens_details")
                                .and_then(|d| d.get("cached_tokens"))
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            events.push(ModelStreamEvent::Usage {
                                usage: Usage {
                                    input_tokens: input.saturating_sub(cached),
                                    output_tokens: output,
                                    cache_read_tokens: cached,
                                    cache_write_tokens: 0,
                                },
                            });
                        }

                        for event in events {
                            yield event;
                        }
                    }
                }

                // Stream closed. If tool calls started but the provider never reported
                // finish_reason, normalize stop to ToolUse so the agent loop continues.
                if any_tool_call && stop_reason == StopReason::Stop {
                    stop_reason = StopReason::ToolUse;
                }

                for event in flush_tool_calls(&tool_calls) {
                    yield event;
                }
                yield ModelStreamEvent::Done { stop_reason };
            };

            Ok::<ModelStream, ModelError>(Box::pin(stream))
        })
    }
}

fn backoff_duration(attempt: u32) -> Duration {
    Duration::from_millis(BASE_DELAY_MS.saturating_mul(2_u64.saturating_pow(attempt)))
}

fn should_retry_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn normalize_http_error(status: StatusCode, text: &str) -> ModelError {
    let kind = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ModelErrorKind::Authentication,
        StatusCode::TOO_MANY_REQUESTS => ModelErrorKind::RateLimited,
        StatusCode::BAD_REQUEST => ModelErrorKind::InvalidRequest,
        StatusCode::INTERNAL_SERVER_ERROR
        | StatusCode::BAD_GATEWAY
        | StatusCode::SERVICE_UNAVAILABLE
        | StatusCode::GATEWAY_TIMEOUT => ModelErrorKind::Transport,
        _ => ModelErrorKind::Provider,
    };
    ModelError::new(kind, text)
}

fn map_finish_reason(reason: &str) -> StopReason {
    match reason {
        "tool_calls" => StopReason::ToolUse,
        "stop" => StopReason::Stop,
        "length" => StopReason::Length,
        "content_filter" | "error" => StopReason::Error,
        _ => StopReason::Stop,
    }
}

#[derive(Default)]
struct ToolCallBuffer {
    id: String,
    name: String,
    arguments: String,
    started: bool,
}

fn flush_tool_calls(tool_calls: &BTreeMap<usize, ToolCallBuffer>) -> Vec<ModelStreamEvent> {
    let mut events = Vec::new();
    for (index, call) in tool_calls.iter() {
        if !call.started {
            continue;
        }
        let arguments = serde_json::from_str(&call.arguments).unwrap_or_else(|_| json!({}));
        events.push(ModelStreamEvent::ToolCallEnd {
            content_index: *index,
            id: call.id.clone(),
            name: call.name.clone(),
            arguments,
        });
    }
    events
}

fn sse_data(frame: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in frame.lines() {
        let line = line.trim_start();
        if line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            lines.push(rest.trim_start());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn build_request_body(request: ModelRequest, model_id: &str) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = request
        .system_prompt
        .filter(|prompt| !prompt.trim().is_empty())
    {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for message in request.messages {
        match message {
            ModelMessage::User { content } => {
                messages.push(json!({
                    "role": "user",
                    "content": convert_user_content(content),
                }));
            }
            ModelMessage::Assistant { content } => {
                messages.push(convert_assistant(content));
            }
            ModelMessage::ToolResult {
                tool_call_id,
                content,
                ..
            } => {
                let text = content
                    .into_iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": text,
                }));
            }
        }
    }

    let tools = if request.tools.is_empty() {
        Value::Null
    } else {
        Value::Array(request.tools.into_iter().map(convert_tool).collect())
    };

    let mut body = json!({
        "model": model_id,
        "messages": messages,
        "stream": true,
        "tool_choice": "auto",
    });
    if !tools.is_null() {
        body["tools"] = tools;
    }
    if let Some(temperature) = request.params.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(max_tokens) = request.params.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(reasoning) = request.params.reasoning {
        body["reasoning"] = json!({ "effort": reasoning.code(), "exclude": false });
    }
    body
}

fn convert_tool(tool: ModelToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        },
    })
}

fn convert_user_content(content: Vec<ContentBlock>) -> Value {
    let mut parts = Vec::new();
    for block in content {
        match block {
            ContentBlock::Text { text } => {
                parts.push(json!({ "type": "text", "text": text }));
            }
            ContentBlock::Image { data, mime_type } => {
                parts.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{mime_type};base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)),
                    },
                }));
            }
            _ => {}
        }
    }
    Value::Array(parts)
}

fn convert_assistant(content: Vec<ContentBlock>) -> Value {
    let mut text = Vec::new();
    let mut tool_calls = Vec::new();
    for block in content {
        match block {
            ContentBlock::Text { text: t } => {
                if !t.is_empty() {
                    text.push(t);
                }
            }
            ContentBlock::ToolCall {
                id,
                name,
                arguments,
            } => {
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": arguments.to_string() },
                }));
            }
            ContentBlock::Thinking { .. } | ContentBlock::Image { .. } => {}
        }
    }

    let mut message = json!({ "role": "assistant" });
    if tool_calls.is_empty() {
        message["content"] = Value::String(text.join("\n"));
    } else {
        message["content"] = if text.is_empty() {
            Value::Null
        } else {
            Value::String(text.join("\n"))
        };
        message["tool_calls"] = Value::Array(tool_calls);
    }
    message
}


