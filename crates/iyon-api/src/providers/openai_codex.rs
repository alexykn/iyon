use std::{sync::Arc, time::Duration};

use async_stream::try_stream;
use futures_util::StreamExt;
use reqwest::{
    Response, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde_json::{Value, json};

use crate::{
    CacheRetention, ContentBlock, ModelApi, ModelError, ModelErrorKind, ModelMessage, ModelRequest,
    ModelStream, ModelStreamEvent, ModelStreamFuture, ModelToolSpec, ReasoningLevel, StopReason,
    Usage,
};

const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api";
const DEFAULT_MODEL: &str = "gpt-5.3-codex";
const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 400;

#[derive(Clone)]
pub struct OpenAICodexModelApi {
    client: reqwest::Client,
    access_token: Arc<str>,
    account_id: Arc<str>,
    base_url: Arc<str>,
}

impl OpenAICodexModelApi {
    pub fn new(
        access_token: impl Into<String>,
        account_id: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("accept", HeaderValue::from_static("text/event-stream"));
        headers.insert(
            "OpenAI-Beta",
            HeaderValue::from_static("responses=experimental"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| ModelError::new(ModelErrorKind::Transport, e.to_string()))?;

        Ok(Self {
            client,
            access_token: access_token.into().into(),
            account_id: account_id.into().into(),
            base_url: DEFAULT_BASE_URL.into(),
        })
    }

    fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/codex/responses") {
            base.to_string()
        } else if base.ends_with("/codex") {
            format!("{base}/responses")
        } else {
            format!("{base}/codex/responses")
        }
    }

    async fn send_with_retry(
        &self,
        body: &Value,
        session_id: &str,
    ) -> Result<Response, ModelError> {
        let url = self.endpoint();
        let bearer = format!("Bearer {}", self.access_token);
        let mut last_error: Option<ModelError> = None;

        for attempt in 0..=MAX_RETRIES {
            let result = self
                .client
                .post(&url)
                .header(AUTHORIZATION, &bearer)
                .header("chatgpt-account-id", self.account_id.as_ref())
                .header("originator", "iyon")
                .header("session_id", session_id)
                .header("x-client-request-id", session_id)
                .json(body)
                .send()
                .await;

            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    let mapped = ModelError::new(
                        ModelErrorKind::Transport,
                        format!("codex transport error (attempt {}): {error}", attempt + 1),
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
            let mapped = normalize_http_error(status, &text);

            if should_retry_status(status) && attempt < MAX_RETRIES {
                tokio::time::sleep(backoff_duration(attempt)).await;
                last_error = Some(mapped);
                continue;
            }
            return Err(mapped);
        }

        Err(last_error.unwrap_or_else(|| ModelError::unknown("codex request failed after retries")))
    }
}

impl ModelApi for OpenAICodexModelApi {
    fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_> {
        Box::pin(async move {
            let session_id = request
                .metadata
                .session_id
                .clone()
                .unwrap_or_else(|| format!("iyon_{}", uuid_like()));
            let body = build_request_body(request, &session_id);
            let response = self.send_with_retry(&body, &session_id).await?;

            let stream = try_stream! {
                yield ModelStreamEvent::Started;

                let mut bytes = response.bytes_stream();
                let mut buffer = String::new();
                let mut text_blocks: Vec<String> = Vec::new();
                let mut thinking_blocks: Vec<String> = Vec::new();
                let mut tool_calls: Vec<(Option<String>, Option<String>, String)> = Vec::new();
                let mut saw_tool_call = false;
                let mut stop_reason = StopReason::Stop;

                while let Some(chunk) = bytes.next().await {
                    let chunk = chunk.map_err(|e| ModelError::new(ModelErrorKind::Transport, e.to_string()))?;
                    buffer.push_str(&String::from_utf8_lossy(&chunk));

                    while let Some(idx) = buffer.find("\n\n") {
                        let frame = buffer[..idx].to_string();
                        buffer = buffer[idx + 2..].to_string();
                        let Some(data) = sse_data(&frame) else { continue; };
                        if data == "[DONE]" { continue; }

                        let event: Value = serde_json::from_str(&data)
                            .map_err(|e| ModelError::new(ModelErrorKind::Provider, format!("invalid codex event json: {e}")))?;
                        let Some(kind) = event.get("type").and_then(Value::as_str) else { continue; };

                        match kind {
                            "response.output_item.added" => handle_output_item_added(&event, &mut text_blocks, &mut thinking_blocks, &mut tool_calls),
                            "response.output_text.delta" | "response.refusal.delta" => {
                                if let Some(delta) = event.get("delta").and_then(Value::as_str)
                                    && let Some(last) = text_blocks.last_mut() {
                                        last.push_str(delta);
                                        yield ModelStreamEvent::TextDelta {
                                            content_index: text_blocks.len().saturating_sub(1),
                                            delta: delta.to_string(),
                                        };
                                    }
                            }
                            "response.reasoning_summary_text.delta" => {
                                if let Some(delta) = event.get("delta").and_then(Value::as_str)
                                    && let Some(last) = thinking_blocks.last_mut() {
                                        last.push_str(delta);
                                        yield ModelStreamEvent::ThinkingDelta {
                                            content_index: thinking_blocks.len().saturating_sub(1),
                                            delta: delta.to_string(),
                                        };
                                    }
                            }
                            "response.reasoning_summary_part.done" => {
                                if let Some(last) = thinking_blocks.last_mut() {
                                    last.push_str("\n\n");
                                    yield ModelStreamEvent::ThinkingDelta {
                                        content_index: thinking_blocks.len().saturating_sub(1),
                                        delta: "\n\n".to_string(),
                                    };
                                }
                            }
                            "response.function_call_arguments.delta" => {
                                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                                    let index = tool_calls.len().saturating_sub(1);
                                    if let Some((id, name, partial)) = tool_calls.last_mut() {
                                        partial.push_str(delta);
                                        yield ModelStreamEvent::ToolCallDelta {
                                            content_index: index,
                                            id: id.clone(),
                                            name: name.clone(),
                                            arguments_delta: delta.to_string(),
                                        };
                                    }
                                }
                            }
                            "response.function_call_arguments.done" => {
                                if let Some(arguments) = event.get("arguments").and_then(Value::as_str)
                                    && let Some((_, _, partial)) = tool_calls.last_mut() {
                                        if arguments.starts_with(partial.as_str()) {
                                            let delta = &arguments[partial.len()..];
                                            *partial = arguments.to_string();
                                            if !delta.is_empty() {
                                                let index = tool_calls.len().saturating_sub(1);
                                                if let Some((id, name, _)) = tool_calls.last() {
                                                    yield ModelStreamEvent::ToolCallDelta {
                                                        content_index: index,
                                                        id: id.clone(),
                                                        name: name.clone(),
                                                        arguments_delta: delta.to_string(),
                                                    };
                                                }
                                            }
                                        } else {
                                            *partial = arguments.to_string();
                                        }
                                    }
                            }
                            "response.output_item.done" => {
                                if let Some(item) = event.get("item") {
                                    match item.get("type").and_then(Value::as_str) {
                                        Some("message") => {
                                            if let Some(last) = text_blocks.last() {
                                                yield ModelStreamEvent::TextEnd {
                                                    content_index: text_blocks.len().saturating_sub(1),
                                                    text: last.clone(),
                                                };
                                            }
                                        }
                                        Some("reasoning") => {
                                            if let Some(last) = thinking_blocks.last() {
                                                yield ModelStreamEvent::ThinkingEnd {
                                                    content_index: thinking_blocks.len().saturating_sub(1),
                                                    text: last.clone(),
                                                };
                                            }
                                        }
                                        Some("function_call") => {
                                            if let Some((id, name, partial)) = tool_calls.last() {
                                                let Some(id) = id.clone() else { continue; };
                                                let Some(name) = name.clone() else { continue; };
                                                let arguments: Value = serde_json::from_str(partial).unwrap_or_else(|_| json!({}));
                                                saw_tool_call = true;
                                                yield ModelStreamEvent::ToolCallEnd {
                                                    content_index: tool_calls.len().saturating_sub(1),
                                                    id,
                                                    name,
                                                    arguments,
                                                };
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "response.completed" | "response.done" | "response.incomplete" => {
                                if let Some(response) = event.get("response") {
                                    if let Some(usage) = response.get("usage") {
                                        let input_tokens = usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
                                        let output_tokens = usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
                                        let cache_read_tokens = usage
                                            .get("input_tokens_details")
                                            .and_then(|d| d.get("cached_tokens"))
                                            .and_then(Value::as_u64)
                                            .unwrap_or(0);
                                        yield ModelStreamEvent::Usage {
                                            usage: Usage {
                                                input_tokens: input_tokens.saturating_sub(cache_read_tokens),
                                                output_tokens,
                                                cache_read_tokens,
                                                cache_write_tokens: 0,
                                            },
                                        };
                                    }
                                    stop_reason = map_stop_reason(response.get("status").and_then(Value::as_str));
                                }
                            }
                            "response.failed" => {
                                let message = event
                                    .get("response")
                                    .and_then(|r| r.get("error"))
                                    .and_then(|e| e.get("message"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("codex response failed");
                                Err::<(), _>(ModelError::new(ModelErrorKind::Provider, message))?;
                            }
                            "error" => {
                                let message = event.get("message").and_then(Value::as_str).unwrap_or("provider error");
                                Err::<(), _>(ModelError::new(ModelErrorKind::Provider, message))?;
                            }
                            _ => {}
                        }

                        if kind == "response.output_item.added"
                            && let Some(item) = event.get("item") {
                                match item.get("type").and_then(Value::as_str) {
                                    Some("message") => {
                                        yield ModelStreamEvent::TextStart { content_index: text_blocks.len().saturating_sub(1) };
                                    }
                                    Some("reasoning") => {
                                        yield ModelStreamEvent::ThinkingStart { content_index: thinking_blocks.len().saturating_sub(1) };
                                    }
                                    Some("function_call") => {
                                        if let Some((id, name, _)) = tool_calls.last() {
                                            yield ModelStreamEvent::ToolCallStart {
                                                content_index: tool_calls.len().saturating_sub(1),
                                                id: id.clone(),
                                                name: name.clone(),
                                            };
                                        }
                                    }
                                    _ => {}
                                }
                            }
                    }
                }

                if saw_tool_call && stop_reason == StopReason::Stop {
                    stop_reason = StopReason::ToolUse;
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

    let message = if let Ok(value) = serde_json::from_str::<Value>(text) {
        if let Some(detail) = value.get("detail").and_then(Value::as_str) {
            format!("codex request failed ({status}): {detail}")
        } else if let Some(message) = value.get("message").and_then(Value::as_str) {
            format!("codex request failed ({status}): {message}")
        } else {
            format!("codex request failed ({status}): {text}")
        }
    } else {
        format!("codex request failed ({status}): {text}")
    };

    ModelError::new(kind, message)
}

fn handle_output_item_added(
    event: &Value,
    text_blocks: &mut Vec<String>,
    thinking_blocks: &mut Vec<String>,
    tool_calls: &mut Vec<(Option<String>, Option<String>, String)>,
) {
    let Some(item) = event.get("item") else {
        return;
    };
    match item.get("type").and_then(Value::as_str) {
        Some("message") => text_blocks.push(String::new()),
        Some("reasoning") => thinking_blocks.push(String::new()),
        Some("function_call") => {
            let id = item
                .get("call_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let partial = item
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            tool_calls.push((id, name, partial));
        }
        _ => {}
    }
}

fn map_stop_reason(status: Option<&str>) -> StopReason {
    match status.unwrap_or("completed") {
        "completed" => StopReason::Stop,
        "incomplete" => StopReason::Length,
        "failed" => StopReason::Error,
        "cancelled" => StopReason::Aborted,
        _ => StopReason::Stop,
    }
}

fn sse_data(frame: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in frame.lines() {
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

fn build_request_body(request: ModelRequest, session_id: &str) -> Value {
    let input = request
        .messages
        .into_iter()
        .flat_map(convert_message)
        .collect::<Vec<_>>();

    let tools = if request.tools.is_empty() {
        Value::Null
    } else {
        Value::Array(request.tools.into_iter().map(convert_tool).collect())
    };

    let effort = match request.params.reasoning.unwrap_or(ReasoningLevel::Medium) {
        ReasoningLevel::Minimal => "minimal",
        ReasoningLevel::Low => "low",
        ReasoningLevel::Medium => "medium",
        ReasoningLevel::High => "high",
        ReasoningLevel::XHigh => "xhigh",
    };

    let mut body = json!({
        "model": DEFAULT_MODEL,
        "store": false,
        "stream": true,
        "instructions": request.system_prompt.unwrap_or_default(),
        "input": input,
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "text": { "verbosity": "low" },
        "include": ["reasoning.encrypted_content"],
        "prompt_cache_key": session_id,
        "reasoning": { "effort": effort, "summary": "auto" },
    });

    if let Some(temperature) = request.params.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(max_tokens) = request.params.max_tokens {
        body["max_output_tokens"] = json!(max_tokens);
    }
    if !tools.is_null() {
        body["tools"] = tools;
    }
    if matches!(request.params.cache_retention, Some(CacheRetention::None)) {
        body["store"] = json!(false);
    }
    body
}

fn convert_tool(tool: ModelToolSpec) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
        "strict": false,
    })
}

fn convert_message(message: ModelMessage) -> Vec<Value> {
    match message {
        ModelMessage::User { content } => {
            vec![json!({"role":"user","content": convert_content_blocks(content, true)})]
        }
        ModelMessage::Assistant { content } => {
            let mut output = Vec::new();
            for block in content {
                match block {
                    ContentBlock::Text { text } => output.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": text, "annotations": []}],
                        "status": "completed",
                    })),
                    ContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                    } => {
                        output.push(json!({
                            "type": "function_call",
                            "call_id": id,
                            "name": name,
                            "arguments": arguments.to_string(),
                        }));
                    }
                    ContentBlock::Thinking { .. } | ContentBlock::Image { .. } => {}
                }
            }
            output
        }
        ModelMessage::ToolResult {
            tool_call_id,
            content,
            ..
        } => {
            let text = content
                .into_iter()
                .filter_map(|c| match c {
                    ContentBlock::Text { text } => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            vec![json!({
                "type": "function_call_output",
                "call_id": tool_call_id,
                "output": text,
            })]
        }
    }
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{now:032x}")
}

fn convert_content_blocks(content: Vec<ContentBlock>, user: bool) -> Vec<Value> {
    content
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(json!({
                "type": if user { "input_text" } else { "output_text" },
                "text": text,
            })),
            ContentBlock::Image { data, mime_type } if user => Some(json!({
                "type": "input_image",
                "detail": "auto",
                "image_url": format!("data:{mime_type};base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)),
            })),
            _ => None,
        })
        .collect()
}
