use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};

use iyon_core::{
    CoreEvent,
    ids::{MessageId, TurnId},
    kernel::{ModelTurn as NativeModelTurn, ModelTurnError, ModelTurnResult},
};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    NativeError, api,
    core::{KernelSession, SessionState, message_value},
};

const MAX_BATCH_SIZE: usize = 256;

#[napi]
pub struct ModelTurn {
    pub(crate) state: Arc<SessionState>,
    turn: StdMutex<Option<NativeModelTurn>>,
    cancellation: CancellationToken,
    cancelled: AtomicBool,
}

impl ModelTurn {
    pub(crate) fn new(
        state: Arc<SessionState>,
        turn_id: TurnId,
        message_id: MessageId,
    ) -> Result<Self> {
        let mut turn = NativeModelTurn::begin(turn_id, message_id);
        let initial_events = turn.take_events();
        for event in initial_events {
            state.try_emit(event)?;
        }
        Ok(Self {
            state,
            turn: StdMutex::new(Some(turn)),
            cancellation: CancellationToken::new(),
            cancelled: AtomicBool::new(false),
        })
    }

    fn take_turn(&self) -> Result<std::sync::MutexGuard<'_, Option<NativeModelTurn>>> {
        self.turn
            .lock()
            .map_err(|_| NativeError::internal("model turn lock is poisoned"))
    }

    async fn send_event(&self, event: CoreEvent, observe_turn_cancel: bool) -> Result<()> {
        self.state.ensure_open()?;
        let sender = self
            .state
            .sender
            .lock()
            .map_err(|_| NativeError::internal("event sender lock is poisoned"))?
            .clone()
            .ok_or_else(NativeError::closed)?;
        let send = sender.send(crate::events::core_event(&event));
        tokio::pin!(send);
        if observe_turn_cancel {
            tokio::select! {
                result = &mut send => result.map_err(|_| NativeError::closed()),
                _ = self.state.cancellation.cancelled() => Err(NativeError::cancelled()),
                _ = self.cancellation.cancelled() => Err(NativeError::cancelled()),
            }
        } else {
            tokio::select! {
                result = &mut send => result.map_err(|_| NativeError::closed()),
                _ = self.state.cancellation.cancelled() => Err(NativeError::cancelled()),
            }
        }
    }

    async fn emit_events(&self, events: Vec<CoreEvent>) -> Result<()> {
        for event in events {
            self.send_event(event, false).await?;
        }
        Ok(())
    }

    async fn emit_interruptible(&self, events: Vec<CoreEvent>) -> Result<()> {
        for event in events {
            self.send_event(event, true).await?;
        }
        Ok(())
    }

    fn result_value(result: &ModelTurnResult) -> Value {
        serde_json::json!({
            "turnId": result.turn_id.0,
            "assistantMessage": message_value(&result.assistant_message),
            "toolCalls": result.tool_calls.iter().filter_map(|call| match call {
                iyon_core::kernel::ToolCallRequest::Ready(call) => Some(serde_json::json!({
                    "id": call.id.0, "name": call.name, "arguments": call.arguments,
                })),
                iyon_core::kernel::ToolCallRequest::Invalid(_) => None,
            }).collect::<Vec<_>>(),
            "stopReason": match result.stop_reason {
                iyon_api::StopReason::Stop => "stop",
                iyon_api::StopReason::Length => "length",
                iyon_api::StopReason::ToolUse => "toolUse",
                iyon_api::StopReason::Error => "error",
                iyon_api::StopReason::Aborted => "aborted",
            },
            "cancelled": result.cancelled,
        })
    }

    fn turn_error(error: ModelTurnError) -> napi::Error {
        NativeError::invalid_input(error.to_string())
    }

    fn append_result(&self, result: &ModelTurnResult) -> Result<()> {
        self.state
            .session
            .lock()
            .map_err(|_| NativeError::internal("session lock is poisoned"))?
            .append_message(result.assistant_message.clone())
            .map(|_| ())
            .map_err(|error| NativeError::internal(error.to_string()))
    }

    async fn settle(&self, result: ModelTurnResult, terminal: CoreEvent) -> Result<Value> {
        self.append_result(&result)?;
        let events = {
            let mut turn = self.take_turn()?;
            turn.as_mut()
                .ok_or_else(|| NativeError::closed())?
                .take_events()
        };
        self.emit_events(events).await?;
        self.emit_events(vec![terminal]).await?;
        Ok(Self::result_value(&result))
    }

    fn settle_cancelled(&self, result: ModelTurnResult, terminal: CoreEvent) -> Result<Value> {
        self.append_result(&result)?;
        let events = {
            let mut turn = self.take_turn()?;
            turn.as_mut().ok_or_else(NativeError::closed)?.take_events()
        };
        // Cancellation must settle even when the consumer has left the event
        // channel backpressured. Buffered events remain available; a full
        // queue is allowed to drop only this cancellation notification.
        for event in events {
            let _ = self.state.try_emit(event);
        }
        let _ = self.state.try_emit(terminal);
        Ok(Self::result_value(&result))
    }
}

#[napi]
impl ModelTurn {
    #[napi]
    pub async fn push(&self, value: Value) -> Result<()> {
        self.state.ensure_open()?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(NativeError::cancelled());
        }
        let event = api::stream_event(value)?;
        let events = {
            let mut turn = self.take_turn()?;
            let turn = turn.as_mut().ok_or_else(NativeError::closed)?;
            turn.push(event).map_err(Self::turn_error)?;
            turn.take_events()
        };
        self.emit_interruptible(events).await
    }

    #[napi(js_name = "pushMany")]
    pub async fn push_many(&self, values: Vec<Value>) -> Result<()> {
        if values.len() > MAX_BATCH_SIZE {
            return Err(NativeError::invalid_input(format!(
                "pushMany accepts at most {MAX_BATCH_SIZE} events"
            )));
        }
        for value in values {
            self.push(value).await?;
        }
        Ok(())
    }

    #[napi]
    pub async fn finish(&self) -> Result<Value> {
        self.state.ensure_open()?;
        let result = {
            let mut turn = self.take_turn()?;
            turn.as_mut()
                .ok_or_else(NativeError::closed)?
                .finish()
                .map_err(Self::turn_error)?
        };
        let turn_id = result.turn_id.0;
        self.settle(result, CoreEvent::TurnFinished { turn_id })
            .await
    }

    #[napi]
    pub async fn fail(&self, error: Value) -> Result<()> {
        self.state.ensure_open()?;
        let message = if let Some(message) = error.as_str() {
            message.to_owned()
        } else {
            api::model_error(error)?.message
        };
        let turn_id = {
            let mut turn = self.take_turn()?;
            let turn = turn.as_mut().ok_or_else(NativeError::closed)?;
            let turn_id = turn.turn_id().0;
            turn.fail(message.clone());
            turn_id
        };
        self.emit_events(vec![CoreEvent::TurnFailed { turn_id, message }])
            .await
    }

    #[napi]
    pub async fn cancel(&self) -> Result<Value> {
        self.state.ensure_open()?;
        self.cancelled.store(true, Ordering::Release);
        self.cancellation.cancel();
        let result = {
            let mut turn = self.take_turn()?;
            turn.as_mut()
                .ok_or_else(NativeError::closed)?
                .cancel()
                .map_err(Self::turn_error)?
        };
        let turn_id = result.turn_id.0;
        self.settle_cancelled(result, CoreEvent::TurnCancelled { turn_id })
    }
}

impl Drop for ModelTurn {
    fn drop(&mut self) {
        // Dropping the JS Promise or wrapper is not cancellation. Only the
        // explicit cancel method or the owning session's abort closes work.
    }
}

pub(crate) fn begin_session_turn(session: &KernelSession, request: Value) -> Result<ModelTurn> {
    session.state.ensure_open()?;
    let _request = api::model_request(request)?;
    let (turn_id, message_id) = {
        let session_guard = session
            .state
            .session
            .lock()
            .map_err(|_| NativeError::internal("session lock is poisoned"))?;
        let turn_id = session.state.next_turn.fetch_add(1, Ordering::AcqRel);
        (TurnId(turn_id), MessageId(session_guard.next_message_id()))
    };
    ModelTurn::new(Arc::clone(&session.state), turn_id, message_id)
}
