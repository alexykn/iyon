use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use iyon_core::{
    CoreEvent,
    ids::{MessageId, SessionId, ToolCallId as NativeToolCallId},
    kernel::{
        AgentMessage, ApprovalRequirement, ContentBlock, KernelQueues,
        KernelSession as NativeSession, SessionEntry, SessionMessageError, SessionSnapshot,
    },
};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use serde_json::{Map, Value, json};
use tokio::sync::{Mutex, Notify, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{NativeError, api, events};

const DEFAULT_EVENT_CAPACITY: usize = 128;

pub(crate) struct SessionState {
    pub(crate) session: StdMutex<NativeSession>,
    pub(crate) queues: StdMutex<KernelQueues>,
    pub(crate) sender: StdMutex<Option<mpsc::Sender<Value>>>,
    pub(crate) receiver: Mutex<mpsc::Receiver<Value>>,
    pub(crate) closed: AtomicBool,
    pub(crate) close_notify: Notify,
    pub(crate) cancellation: CancellationToken,
    pub(crate) next_turn: AtomicU64,
}

impl SessionState {
    fn new(session: NativeSession, capacity: usize) -> Arc<Self> {
        let cancellation = CancellationToken::new();
        let (sender, receiver) = mpsc::channel(capacity.max(1));
        Arc::new(Self {
            session: StdMutex::new(session),
            queues: StdMutex::new(KernelQueues::new(capacity.max(1), cancellation.clone())),
            sender: StdMutex::new(Some(sender)),
            receiver: Mutex::new(receiver),
            closed: AtomicBool::new(false),
            close_notify: Notify::new(),
            cancellation,
            next_turn: AtomicU64::new(1),
        })
    }

    pub(crate) fn ensure_open(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(NativeError::closed());
        }
        Ok(())
    }

    pub(crate) fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.cancellation.cancel();
            if let Ok(mut sender) = self.sender.lock() {
                sender.take();
            }
            self.close_notify.notify_waiters();
        }
    }

    pub(crate) async fn emit(&self, event: CoreEvent) -> Result<()> {
        self.ensure_open()?;
        let sender = self
            .sender
            .lock()
            .map_err(|_| NativeError::internal("event sender lock is poisoned"))?
            .clone()
            .ok_or_else(NativeError::closed)?;
        sender
            .send(events::core_event(&event))
            .await
            .map_err(|_| NativeError::closed())
    }

    pub(crate) fn try_emit(&self, event: CoreEvent) -> Result<()> {
        self.ensure_open()?;
        let sender = self
            .sender
            .lock()
            .map_err(|_| NativeError::internal("event sender lock is poisoned"))?
            .clone()
            .ok_or_else(NativeError::closed)?;
        sender
            .try_send(events::core_event(&event))
            .map_err(|_| NativeError::internal("session event queue is full"))
    }
}

#[napi]
pub struct KernelSession {
    pub(crate) state: Arc<SessionState>,
}

#[napi]
impl KernelSession {
    #[napi(constructor)]
    pub fn new(options: Option<Value>) -> Result<Self> {
        let options = options
            .map(|value| crate::value::object(value, "session options"))
            .transpose()?
            .unwrap_or_default();
        let id = options
            .get("id")
            .map(|value| {
                value.as_u64().ok_or_else(|| {
                    NativeError::invalid_input("session id must be a non-negative integer")
                })
            })
            .transpose()?
            .unwrap_or(1);
        let capacity = options
            .get("eventCapacity")
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| NativeError::invalid_input("eventCapacity must fit in usize"))
            })
            .transpose()?
            .unwrap_or(DEFAULT_EVENT_CAPACITY);
        Ok(Self {
            state: SessionState::new(NativeSession::new(SessionId(id)), capacity),
        })
    }

    #[napi]
    pub fn snapshot(&self) -> Result<Value> {
        self.state.ensure_open()?;
        let session = self
            .state
            .session
            .lock()
            .map_err(|_| NativeError::internal("session lock is poisoned"))?;
        Ok(snapshot_value(&session.snapshot()))
    }

    #[napi(js_name = "beginModelTurn")]
    pub fn begin_model_turn(&self, options: Value) -> Result<crate::model_turn::ModelTurn> {
        let object = crate::value::object(options, "model turn options")?;
        let request = object
            .get("request")
            .cloned()
            .ok_or_else(|| NativeError::invalid_input("model turn request is required"))?;
        crate::model_turn::begin_session_turn(self, request)
    }

    #[napi(js_name = "prepareToolExecution")]
    pub fn prepare_tool_execution(
        &self,
        request: Value,
    ) -> Result<crate::tool_execution::ToolExecution> {
        self.state.ensure_open()?;
        let object = crate::value::object(request, "tool execution request")?;
        let turn_id = crate::value::required_u64(&object, "turnId")?;
        let message_id = crate::value::required_u64(&object, "messageId")?;
        let tool_call_id = crate::value::required_string(&object, "toolCallId")?;
        let tool_name = crate::value::required_string(&object, "toolName")?;
        let arguments = object
            .get("arguments")
            .cloned()
            .ok_or_else(|| NativeError::invalid_input("tool arguments are required"))?;
        Ok(crate::tool_execution::ToolExecution::new(
            Arc::clone(&self.state),
            iyon_core::ids::TurnId(turn_id),
            iyon_core::ids::MessageId(message_id),
            iyon_core::ids::ToolCallId(tool_call_id),
            tool_name,
            arguments,
        ))
    }

    #[napi(js_name = "appendMessage")]
    pub fn append_message(&self, value: Value) -> Result<f64> {
        self.state.ensure_open()?;
        let message = message_from_value(value)?;
        let mut session = self
            .state
            .session
            .lock()
            .map_err(|_| NativeError::internal("session lock is poisoned"))?;
        session
            .append_message(message)
            .map(|id| id.0 as f64)
            .map_err(session_error)
    }

    #[napi(js_name = "appendEntry")]
    pub fn append_entry(&self, value: Value) -> Result<()> {
        self.state.ensure_open()?;
        let object = crate::value::object(value, "session entry")?;
        let entry = match object
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeError::invalid_input("session entry kind must be a string"))?
        {
            "custom" => SessionEntry::Custom {
                namespace: crate::value::required_string(&object, "namespace")?,
                data: object
                    .get("data")
                    .cloned()
                    .ok_or_else(|| NativeError::invalid_input("custom entry data is required"))?,
            },
            "message" => SessionEntry::Message(message_from_object(object)?),
            other => {
                return Err(NativeError::invalid_input(format!(
                    "unknown session entry type `{other}`"
                )));
            }
        };
        let mut session = self
            .state
            .session
            .lock()
            .map_err(|_| NativeError::internal("session lock is poisoned"))?;
        session.append_entry(entry).map_err(session_error)
    }

    #[napi(js_name = "nextEvent")]
    pub async fn next_event(&self) -> Result<Option<Value>> {
        let mut receiver = self.state.receiver.lock().await;
        loop {
            if let Ok(event) = receiver.try_recv() {
                return Ok(Some(event));
            }
            if self.state.closed.load(Ordering::Acquire) {
                return Ok(None);
            }
            let notified = self.state.close_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.state.closed.load(Ordering::Acquire) {
                continue;
            }
            tokio::select! {
                event = receiver.recv() => return Ok(event),
                _ = &mut notified => {}
            }
        }
    }

    #[napi]
    pub fn enqueue(&self, kind: String, text: String) -> Result<()> {
        self.state.ensure_open()?;
        let mut queues = self
            .state
            .queues
            .lock()
            .map_err(|_| NativeError::internal("queue lock is poisoned"))?;
        let result = match kind.as_str() {
            "prompt" => queues.prompt(text.clone()),
            "steer" => queues.steer(text.clone()),
            "followUp" => queues.follow_up(text.clone()),
            other => {
                return Err(NativeError::invalid_input(format!(
                    "unknown queue kind `{other}`"
                )));
            }
        };
        result.map_err(|error| NativeError::invalid_input(error.to_string()))?;
        if kind == "steer" {
            drop(queues);
            self.state.try_emit(CoreEvent::SteerQueued { text })?;
        }
        Ok(())
    }

    #[napi]
    pub fn dequeue(&self, kind: String) -> Result<Option<String>> {
        self.state.ensure_open()?;
        let mut queues = self
            .state
            .queues
            .lock()
            .map_err(|_| NativeError::internal("queue lock is poisoned"))?;
        match kind.as_str() {
            "prompt" => Ok(queues.take_prompt()),
            "steer" => Ok(queues.drain_steers_at_boundary().into_iter().next()),
            "followUp" => Ok(queues.drain_follow_ups_after_settle().into_iter().next()),
            other => Err(NativeError::invalid_input(format!(
                "unknown queue kind `{other}`"
            ))),
        }
    }

    #[napi(js_name = "queueSnapshot")]
    pub fn queue_snapshot(&self) -> Result<Value> {
        self.state.ensure_open()?;
        let queues = self
            .state
            .queues
            .lock()
            .map_err(|_| NativeError::internal("queue lock is poisoned"))?;
        Ok(json!({
            "pendingPrompts": queues.pending_prompts(),
            "pendingSteers": queues.pending_steers(),
            "pendingFollowUps": queues.pending_follow_ups(),
            "abortRequested": queues.abort_requested(),
        }))
    }

    #[napi]
    pub fn abort(&self) -> Result<()> {
        self.state.ensure_open()?;
        self.state
            .queues
            .lock()
            .map_err(|_| NativeError::internal("queue lock is poisoned"))?
            .abort();
        Ok(())
    }

    #[napi]
    pub fn close(&self) {
        self.state.close();
    }
}

impl Drop for KernelSession {
    fn drop(&mut self) {
        self.state.close();
    }
}

pub(crate) fn message_from_value(value: Value) -> Result<AgentMessage> {
    message_from_object(crate::value::object(value, "transcript message")?)
}

fn message_from_object(object: Map<String, Value>) -> Result<AgentMessage> {
    let id = object
        .get("id")
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| NativeError::invalid_input("message id must be an integer"))
        })
        .transpose()?
        .unwrap_or(0);
    let content = crate::value::array(&object, "content")?
        .into_iter()
        .map(api::content_block)
        .collect::<Result<Vec<ContentBlock>>>()?;
    let timestamp = std::time::SystemTime::now();
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeError::invalid_input("transcript role must be a string"))?;
    match role {
        "user" => Ok(AgentMessage::User {
            id: MessageId(id),
            content,
            timestamp,
        }),
        "assistant" => Ok(AgentMessage::Assistant {
            id: MessageId(id),
            content,
            usage: object.get("usage").cloned().map(api::usage).transpose()?,
            stop_reason: object
                .get("stopReason")
                .and_then(Value::as_str)
                .map(api::stop_reason)
                .transpose()?,
            timestamp,
        }),
        "toolResult" => Ok(AgentMessage::ToolResult {
            id: MessageId(id),
            tool_call_id: NativeToolCallId(crate::value::required_string(&object, "toolCallId")?),
            tool_name: crate::value::required_string(&object, "toolName")?,
            content,
            details: object.get("details").cloned().unwrap_or(Value::Null),
            is_error: object
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            timestamp,
        }),
        "status" => Ok(AgentMessage::Status {
            id: MessageId(id),
            text: crate::value::required_string(&object, "text")?,
            timestamp,
        }),
        other => Err(NativeError::invalid_input(format!(
            "unknown transcript role `{other}`"
        ))),
    }
}

pub(crate) fn message_value(message: &AgentMessage) -> Value {
    match message {
        AgentMessage::User {
            id,
            content,
            timestamp,
        } => json!({
            "kind": "message", "role": "user", "id": id.0,
            "content": content.iter().map(content_value).collect::<Vec<_>>(),
            "timestamp": timestamp_value(timestamp),
        }),
        AgentMessage::Assistant {
            id,
            content,
            usage,
            stop_reason,
            timestamp,
        } => json!({
            "kind": "message", "role": "assistant", "id": id.0,
            "content": content.iter().map(content_value).collect::<Vec<_>>(),
            "usage": usage.map(|value| usage_value(&value)),
            "stopReason": stop_reason.map(|value| stop_reason_value(&value)),
            "timestamp": timestamp_value(timestamp),
        }),
        AgentMessage::ToolResult {
            id,
            tool_call_id,
            tool_name,
            content,
            details,
            is_error,
            timestamp,
        } => json!({
            "kind": "message", "role": "toolResult", "id": id.0,
            "toolCallId": tool_call_id.0, "toolName": tool_name,
            "content": content.iter().map(content_value).collect::<Vec<_>>(),
            "details": details, "isError": is_error, "timestamp": timestamp_value(timestamp),
        }),
        AgentMessage::Status {
            id,
            text,
            timestamp,
        } => json!({
            "kind": "message", "role": "status", "id": id.0, "text": text,
            "timestamp": timestamp_value(timestamp),
        }),
    }
}

pub(crate) fn content_value(content: &ContentBlock) -> Value {
    match content {
        ContentBlock::Text { text } => json!({"type": "text", "text": text}),
        ContentBlock::Image { data, mime_type } => {
            json!({"type": "image", "data": data, "mimeType": mime_type})
        }
        ContentBlock::Thinking { text } => json!({"type": "thinking", "text": text}),
        ContentBlock::ToolCall {
            id,
            name,
            arguments,
        } => json!({
            "type": "toolCall", "id": id, "name": name, "arguments": arguments,
        }),
    }
}

fn usage_value(usage: &iyon_api::Usage) -> Value {
    json!({
        "inputTokens": usage.input_tokens, "outputTokens": usage.output_tokens,
        "cacheReadTokens": usage.cache_read_tokens, "cacheWriteTokens": usage.cache_write_tokens,
    })
}

fn stop_reason_value(reason: &iyon_api::StopReason) -> &'static str {
    match reason {
        iyon_api::StopReason::Stop => "stop",
        iyon_api::StopReason::Length => "length",
        iyon_api::StopReason::ToolUse => "toolUse",
        iyon_api::StopReason::Error => "error",
        iyon_api::StopReason::Aborted => "aborted",
    }
}

fn timestamp_value(timestamp: &std::time::SystemTime) -> String {
    timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

pub(crate) fn snapshot_value(snapshot: &SessionSnapshot) -> Value {
    json!({
        "sessionId": snapshot.session_id.0,
        "entries": snapshot.entries.iter().map(|entry| match entry {
            SessionEntry::Message(message) => message_value(message),
            SessionEntry::Custom { namespace, data } => json!({
                "kind": "custom", "namespace": namespace, "data": data,
            }),
        }).collect::<Vec<_>>(),
    })
}

fn session_error(error: SessionMessageError) -> napi::Error {
    NativeError::invalid_input(error.to_string())
}

pub(crate) fn approval_requirement(value: Option<Value>) -> Result<ApprovalRequirement> {
    let Some(value) = value else {
        return Ok(ApprovalRequirement::NotRequired);
    };
    let object = crate::value::object(value, "approval requirement")?;
    match crate::value::discriminant(&object)? {
        "notRequired" => Ok(ApprovalRequirement::NotRequired),
        "required" => Ok(ApprovalRequirement::Required {
            reason: crate::value::optional_string(&object, "reason")?,
        }),
        other => Err(NativeError::invalid_input(format!(
            "unknown approval requirement `{other}`"
        ))),
    }
}
