use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use napi::bindgen_prelude::Result;
use napi_derive::napi;
use serde_json::Value;
use tokio::sync::{Mutex, Notify, mpsc};

use crate::NativeError;

const EVENT_QUEUE_CAPACITY: usize = 64;

struct QueueState {
    sender: std::sync::Mutex<Option<mpsc::Sender<Value>>>,
    receiver: Mutex<mpsc::Receiver<Value>>,
    closed: AtomicBool,
    close_notify: Notify,
}

impl QueueState {
    fn close(&self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        self.closed.store(true, Ordering::Release);
        self.close_notify.notify_waiters();
    }
}

/// The sender and receiver are owned native state. A receiver future holds
/// only an Arc<QueueState>; closing the handle drops the sender and notifies
/// pending operations, so no Tokio task can retain the JS wrapper.
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
                closed: AtomicBool::new(false),
                close_notify: Notify::new(),
            }),
        }
    }

    #[napi]
    pub async fn send(&self, event: Value) -> Result<()> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(NativeError::closed());
        }
        let sender = self
            .state
            .sender
            .lock()
            .map_err(|_| NativeError::internal("queue sender lock is poisoned"))?
            .clone()
            .ok_or_else(NativeError::closed)?;
        let close_notified = self.state.close_notify.notified();
        tokio::pin!(close_notified);
        close_notified.as_mut().enable();
        if self.state.closed.load(Ordering::Acquire) {
            return Err(NativeError::closed());
        }
        tokio::select! {
            result = sender.send(event) => result.map_err(|_| NativeError::closed()),
            _ = &mut close_notified => Err(NativeError::closed()),
        }
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

            let close_notified = self.state.close_notify.notified();
            tokio::pin!(close_notified);
            close_notified.as_mut().enable();
            if self.state.closed.load(Ordering::Acquire) {
                continue;
            }
            tokio::select! {
                event = receiver.recv() => return Ok(event),
                _ = &mut close_notified => {}
            }
        }
    }

    #[napi]
    pub fn close(&self) {
        self.state.close();
    }
}

impl Drop for EventQueueProbe {
    fn drop(&mut self) {
        self.state.close();
    }
}
