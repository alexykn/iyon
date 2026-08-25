use iyon_api::{
    CacheRetention, ContentBlock, ModelError, ModelErrorKind, ModelMessage, ModelMetadata,
    ModelParams, ModelRequest, ModelStreamEvent, ModelToolSpec, ReasoningLevel, StopReason, Usage,
};
use serde_json::Value;

use crate::{NativeError, value};

pub(crate) fn model_request(value: Value) -> Result<ModelRequest, napi::Error> {
    let object = value::object(value, "model request")?;
    let params = model_params(value::optional_object(&object, "params")?)?;
    let metadata = model_metadata(value::optional_object(&object, "metadata")?)?;
    let messages = value::array(&object, "messages")?
        .into_iter()
        .map(model_message)
        .collect::<Result<_, _>>()?;
    let tools = value::optional_array(&object, "tools")?
        .into_iter()
        .map(model_tool_spec)
        .collect::<Result<_, _>>()?;
    Ok(ModelRequest {
        system_prompt: value::optional_string(&object, "systemPrompt")?,
        messages,
        tools,
        params,
        metadata,
    })
}

fn model_params(object: serde_json::Map<String, Value>) -> Result<ModelParams, napi::Error> {
    let temperature = object
        .get("temperature")
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .map(|value| value as f32)
                .ok_or_else(|| NativeError::invalid_input("temperature must be finite"))
        })
        .transpose()?;
    let max_tokens = object
        .get("maxTokens")
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| NativeError::invalid_input("maxTokens must fit in u32"))
        })
        .transpose()?;
    let reasoning = object.get("reasoning").map(reasoning_level).transpose()?;
    let cache_retention = object
        .get("cacheRetention")
        .map(cache_retention)
        .transpose()?;
    Ok(ModelParams {
        temperature,
        max_tokens,
        reasoning,
        cache_retention,
    })
}

fn model_metadata(object: serde_json::Map<String, Value>) -> Result<ModelMetadata, napi::Error> {
    Ok(ModelMetadata {
        session_id: value::optional_string(&object, "sessionId")?,
        user_id: value::optional_string(&object, "userId")?,
    })
}

fn model_message(value: Value) -> Result<ModelMessage, napi::Error> {
    let object = value::object(value, "model message")?;
    let content = value::array(&object, "content")?
        .into_iter()
        .map(content_block)
        .collect::<Result<_, _>>()?;
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeError::invalid_input("model message role must be a string"))?;
    match role {
        "user" => Ok(ModelMessage::User { content }),
        "assistant" => Ok(ModelMessage::Assistant { content }),
        "toolResult" => Ok(ModelMessage::ToolResult {
            tool_call_id: value::required_string(&object, "toolCallId")?,
            tool_name: value::required_string(&object, "toolName")?,
            content,
            is_error: object
                .get("isError")
                .and_then(Value::as_bool)
                .ok_or_else(|| NativeError::invalid_input("isError must be a boolean"))?,
        }),
        other => Err(NativeError::invalid_input(format!(
            "unknown model message type `{other}`"
        ))),
    }
}

pub(crate) fn content_block(value: Value) -> Result<ContentBlock, napi::Error> {
    let object = value::object(value, "content block")?;
    match value::discriminant(&object)? {
        "text" => Ok(ContentBlock::Text {
            text: value::required_string(&object, "text")?,
        }),
        "thinking" => Ok(ContentBlock::Thinking {
            text: value::required_string(&object, "text")?,
        }),
        "image" => Ok(ContentBlock::Image {
            data: bytes(&object, "data")?,
            mime_type: value::required_string(&object, "mimeType")?,
        }),
        "toolCall" => Ok(ContentBlock::ToolCall {
            id: value::required_string(&object, "id")?,
            name: value::required_string(&object, "name")?,
            arguments: object
                .get("arguments")
                .cloned()
                .ok_or_else(|| NativeError::invalid_input("arguments is required"))?,
        }),
        other => Err(NativeError::invalid_input(format!(
            "unknown content block type `{other}`"
        ))),
    }
}

fn bytes(object: &serde_json::Map<String, Value>, field: &str) -> Result<Vec<u8>, napi::Error> {
    let value = object
        .get(field)
        .ok_or_else(|| NativeError::invalid_input(format!("{field} must be a byte array")))?;
    let values = if let Some(values) = value.as_array() {
        values.clone()
    } else if let Some(buffer) = value.as_object() {
        if buffer.get("type").and_then(Value::as_str) == Some("Buffer") {
            buffer
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .ok_or_else(|| {
                    NativeError::invalid_input(format!("{field} must be a byte array"))
                })?
        } else {
            let mut entries = buffer
                .iter()
                .filter_map(|(key, value)| key.parse::<usize>().ok().map(|index| (index, value)))
                .collect::<Vec<_>>();
            entries.sort_by_key(|(index, _)| *index);
            entries
                .into_iter()
                .map(|(_, value)| value.clone())
                .collect()
        }
    } else {
        return Err(NativeError::invalid_input(format!(
            "{field} must be a byte array"
        )));
    };
    values
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or_else(|| {
                    NativeError::invalid_input(format!("{field} contains an invalid byte"))
                })
        })
        .collect()
}

fn model_tool_spec(value: Value) -> Result<ModelToolSpec, napi::Error> {
    let object = value::object(value, "model tool spec")?;
    Ok(ModelToolSpec {
        name: value::required_string(&object, "name")?,
        description: value::required_string(&object, "description")?,
        input_schema: object
            .get("inputSchema")
            .cloned()
            .ok_or_else(|| NativeError::invalid_input("inputSchema is required"))?,
    })
}

pub(crate) fn stream_event(value: Value) -> Result<ModelStreamEvent, napi::Error> {
    let object = value::object(value, "model stream event")?;
    let content_index = || {
        value::required_u64(&object, "contentIndex").and_then(|value| {
            usize::try_from(value)
                .map_err(|_| NativeError::invalid_input("contentIndex is too large"))
        })
    };
    match value::discriminant(&object)? {
        "started" => Ok(ModelStreamEvent::Started),
        "textStart" => Ok(ModelStreamEvent::TextStart {
            content_index: content_index()?,
        }),
        "textDelta" => Ok(ModelStreamEvent::TextDelta {
            content_index: content_index()?,
            delta: value::required_string(&object, "delta")?,
        }),
        "textEnd" => Ok(ModelStreamEvent::TextEnd {
            content_index: content_index()?,
            text: value::required_string(&object, "text")?,
        }),
        "thinkingStart" => Ok(ModelStreamEvent::ThinkingStart {
            content_index: content_index()?,
        }),
        "thinkingDelta" => Ok(ModelStreamEvent::ThinkingDelta {
            content_index: content_index()?,
            delta: value::required_string(&object, "delta")?,
        }),
        "thinkingEnd" => Ok(ModelStreamEvent::ThinkingEnd {
            content_index: content_index()?,
            text: value::required_string(&object, "text")?,
        }),
        "toolCallStart" => Ok(ModelStreamEvent::ToolCallStart {
            content_index: content_index()?,
            id: value::optional_string(&object, "id")?,
            name: value::optional_string(&object, "name")?,
        }),
        "toolCallDelta" => Ok(ModelStreamEvent::ToolCallDelta {
            content_index: content_index()?,
            id: value::optional_string(&object, "id")?,
            name: value::optional_string(&object, "name")?,
            arguments_delta: value::required_string(&object, "argumentsDelta")?,
        }),
        "toolCallEnd" => Ok(ModelStreamEvent::ToolCallEnd {
            content_index: content_index()?,
            id: value::required_string(&object, "id")?,
            name: value::required_string(&object, "name")?,
            arguments: object
                .get("arguments")
                .cloned()
                .ok_or_else(|| NativeError::invalid_input("arguments is required"))?,
        }),
        "usage" => Ok(ModelStreamEvent::Usage {
            usage: usage(
                object
                    .get("usage")
                    .cloned()
                    .ok_or_else(|| NativeError::invalid_input("usage is required"))?,
            )?,
        }),
        "done" => Ok(ModelStreamEvent::Done {
            stop_reason: stop_reason(&value::required_string(&object, "stopReason")?)?,
        }),
        "error" => Ok(ModelStreamEvent::Error {
            message: value::required_string(&object, "message")?,
        }),
        other => Err(NativeError::invalid_input(format!(
            "unknown model stream event type `{other}`"
        ))),
    }
}

pub(crate) fn usage(value: Value) -> Result<Usage, napi::Error> {
    let object = value::object(value, "usage")?;
    Ok(Usage {
        input_tokens: value::required_u64(&object, "inputTokens")?,
        output_tokens: value::required_u64(&object, "outputTokens")?,
        cache_read_tokens: value::required_u64(&object, "cacheReadTokens")?,
        cache_write_tokens: value::required_u64(&object, "cacheWriteTokens")?,
    })
}

pub(crate) fn stop_reason(value: &str) -> Result<StopReason, napi::Error> {
    match value {
        "stop" => Ok(StopReason::Stop),
        "length" => Ok(StopReason::Length),
        "toolUse" => Ok(StopReason::ToolUse),
        "error" => Ok(StopReason::Error),
        "aborted" => Ok(StopReason::Aborted),
        other => Err(NativeError::invalid_input(format!(
            "unknown stop reason `{other}`"
        ))),
    }
}

pub(crate) fn reasoning_level(value: &Value) -> Result<ReasoningLevel, napi::Error> {
    match value.as_str() {
        Some("none") => Ok(ReasoningLevel::None),
        Some("minimal") => Ok(ReasoningLevel::Minimal),
        Some("low") => Ok(ReasoningLevel::Low),
        Some("medium") => Ok(ReasoningLevel::Medium),
        Some("high") => Ok(ReasoningLevel::High),
        Some("xhigh") => Ok(ReasoningLevel::XHigh),
        Some("max") => Ok(ReasoningLevel::Max),
        Some(other) => Err(NativeError::invalid_input(format!(
            "unknown reasoning level `{other}`"
        ))),
        None => Err(NativeError::invalid_input("reasoning must be a string")),
    }
}

pub(crate) fn cache_retention(value: &Value) -> Result<CacheRetention, napi::Error> {
    match value.as_str() {
        Some("none") => Ok(CacheRetention::None),
        Some("short") => Ok(CacheRetention::Short),
        Some("long") => Ok(CacheRetention::Long),
        Some(other) => Err(NativeError::invalid_input(format!(
            "unknown cache retention `{other}`"
        ))),
        None => Err(NativeError::invalid_input(
            "cacheRetention must be a string",
        )),
    }
}

pub(crate) fn model_error(value: Value) -> Result<ModelError, napi::Error> {
    let object = value::object(value, "model error")?;
    Ok(ModelError {
        kind: match value::required_string(&object, "kind")?.as_str() {
            "invalidRequest" => ModelErrorKind::InvalidRequest,
            "authentication" => ModelErrorKind::Authentication,
            "rateLimited" => ModelErrorKind::RateLimited,
            "provider" => ModelErrorKind::Provider,
            "transport" => ModelErrorKind::Transport,
            "cancelled" => ModelErrorKind::Cancelled,
            "unknown" => ModelErrorKind::Unknown,
            other => {
                return Err(NativeError::invalid_input(format!(
                    "unknown model error kind `{other}`"
                )));
            }
        },
        message: value::required_string(&object, "message")?,
    })
}

#[cfg(test)]
mod tests {
    use super::stream_event;
    use iyon_api::{ModelStreamEvent, StopReason};
    use serde_json::json;

    #[test]
    fn stream_events_use_camel_case_discriminants_and_fields() {
        let event = stream_event(json!({
            "type": "toolCallDelta",
            "contentIndex": 3,
            "id": "call-1",
            "name": "read",
            "argumentsDelta": "{\"path\":\"README\"}"
        }))
        .unwrap();
        assert!(matches!(event, ModelStreamEvent::ToolCallDelta {
            content_index: 3,
            arguments_delta,
            ..
        } if arguments_delta.contains("README")));
    }

    #[test]
    fn all_terminal_stop_reasons_are_mapped() {
        for (wire, expected) in [
            ("stop", StopReason::Stop),
            ("length", StopReason::Length),
            ("toolUse", StopReason::ToolUse),
            ("error", StopReason::Error),
            ("aborted", StopReason::Aborted),
        ] {
            let event = stream_event(json!({
                "type": "done",
                "stopReason": wire,
            }))
            .unwrap();
            assert!(
                matches!(event, ModelStreamEvent::Done { stop_reason } if stop_reason == expected)
            );
        }
    }

    #[test]
    fn malformed_values_are_rejected() {
        let error = stream_event(json!({ "type": "notAnEvent" })).unwrap_err();
        assert!(error.reason.contains("ION_INVALID_INPUT"));
    }

    #[test]
    fn image_bytes_accept_buffer_json_shape() {
        let block = super::content_block(json!({
            "type": "image",
            "data": {"type": "Buffer", "data": [0, 127, 255]},
            "mimeType": "image/png",
        }))
        .unwrap();
        assert!(
            matches!(block, iyon_api::ContentBlock::Image { data, .. } if data == vec![0, 127, 255])
        );
    }
}
