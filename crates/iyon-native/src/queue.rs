use std::sync::Arc;

use napi::bindgen_prelude::Result;
use napi_derive::napi;
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

use crate::NativeError;

const EVENT_QUEUE_CAPACITY: usize = 64;

struct QueueState {
    sender: std::sync::Mutex<Option<mpsc::Sender<Value>>>,
    receiver: Mutex<mpsc::Receiver<Value>>,
}

/// The sender and receiver are owned native state. A receiver future holds
/// only an Arc<QueueState>; closing the handle drops the sender and wakes an
/// idle `recv`, so no Tokio task can retain the JS wrapper.
#[napi]
pub struct EventQueueProbe {
    state: Arc<QueueState>,
}

#[napi]
impl EventQueueProbe {
    #[napi(constructor)]
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(EVENT_QUEUE_CAPACITY);
        Self {
            state: Arc::new(QueueState {
                sender: std::sync::Mutex::new(Some(sender)),
                receiver: Mutex::new(receiver),
            }),
        }
    }

    #[napi]
    pub async fn send(&self, event: Value) -> Result<()> {
        let sender = self
            .state
            .sender
            .lock()
            .map_err(|_| NativeError::internal("queue sender lock is poisoned"))?
            .clone()
            .ok_or_else(NativeError::closed)?;
        sender.send(event).await.map_err(|_| NativeError::closed())
    }

    #[napi(js_name = "nextEvent")]
    pub async fn next_event(&self) -> Result<Option<Value>> {
        let mut receiver = self.state.receiver.lock().await;
        Ok(receiver.recv().await)
    }

    #[napi]
    pub fn close(&self) {
        if let Ok(mut sender) = self.state.sender.lock() {
            sender.take();
        }
    }
}

impl Drop for EventQueueProbe {
    fn drop(&mut self) {
        if let Ok(mut sender) = self.state.sender.lock() {
            sender.take();
        }
    }
}
