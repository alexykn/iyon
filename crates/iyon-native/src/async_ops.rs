use std::sync::Arc;
use std::time::Duration;

use napi::bindgen_prelude::Result;
use napi_derive::napi;
use tokio::sync::Notify;

use crate::NativeError;

const MAX_SLEEP_MS: u64 = 10_000;

pub(crate) async fn sleep_operation(ms: u64) -> Result<String> {
    if ms > MAX_SLEEP_MS {
        return Err(NativeError::invalid_input(format!(
            "delay exceeds {MAX_SLEEP_MS} milliseconds"
        )));
    }

    tokio::time::sleep(Duration::from_millis(ms)).await;
    Ok("slept".to_owned())
}

/// The delay is converted and bounded before Tokio suspends. The returned
/// Promise is rejected through a typed N-API error, never by unwinding Rust.
#[napi(js_name = "asyncSleep")]
pub async fn async_sleep(ms: u32) -> Result<String> {
    sleep_operation(u64::from(ms)).await
}

pub(crate) struct CancellationState {
    cancelled: std::sync::atomic::AtomicBool,
    wake: Notify,
}

impl CancellationState {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: std::sync::atomic::AtomicBool::new(false),
            wake: Notify::new(),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
        self.wake.notify_waiters();
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }
}

pub(crate) async fn cancellation_operation(
    state: Arc<CancellationState>,
    ms: u64,
) -> Result<String> {
    if state.is_cancelled() {
        return Err(NativeError::cancelled());
    }

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(ms)) => Ok("completed".to_owned()),
        _ = state.wake.notified() => Err(NativeError::cancelled()),
    }
}
