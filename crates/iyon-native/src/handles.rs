use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use napi::bindgen_prelude::Result;
use napi_derive::napi;

use crate::async_ops::{CancellationState, cancellation_operation};

static LIVE_COUNTERS: AtomicUsize = AtomicUsize::new(0);
static FINALIZED_COUNTERS: AtomicUsize = AtomicUsize::new(0);

/// A long-lived async operation owns an Arc state token. It never retains a
/// JS object or borrowed N-API value while Tokio is suspended.
#[derive(Clone)]
#[napi]
pub struct CancellationProbe {
    state: Arc<CancellationState>,
}

#[napi]
impl CancellationProbe {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            state: Arc::new(CancellationState::new()),
        }
    }

    #[napi]
    pub fn cancel(&self) {
        self.state.cancel();
    }

    #[napi]
    pub async fn run(&self, ms: u32) -> Result<String> {
        cancellation_operation(Arc::clone(&self.state), u64::from(ms)).await
    }
}

impl Drop for CancellationProbe {
    fn drop(&mut self) {
        self.state.cancel();
    }
}

/// Native-owned counter used to verify class method calls and finalization.
#[napi]
pub struct NativeCounter {
    value: AtomicU32,
}

#[napi]
impl NativeCounter {
    #[napi(constructor)]
    pub fn new() -> Self {
        LIVE_COUNTERS.fetch_add(1, Ordering::AcqRel);
        Self {
            value: AtomicU32::new(0),
        }
    }

    #[napi]
    pub fn increment(&self) -> u32 {
        self.value.fetch_add(1, Ordering::AcqRel) + 1
    }

    #[napi]
    pub fn value(&self) -> u32 {
        self.value.load(Ordering::Acquire)
    }
}

impl Drop for NativeCounter {
    fn drop(&mut self) {
        LIVE_COUNTERS.fetch_sub(1, Ordering::AcqRel);
        FINALIZED_COUNTERS.fetch_add(1, Ordering::AcqRel);
    }
}

#[napi(object)]
pub struct NativeCounterStats {
    pub live: u32,
    pub finalized: u32,
}

#[napi(js_name = "nativeCounterStats")]
pub fn native_counter_stats() -> NativeCounterStats {
    NativeCounterStats {
        live: LIVE_COUNTERS.load(Ordering::Acquire) as u32,
        finalized: FINALIZED_COUNTERS.load(Ordering::Acquire) as u32,
    }
}

#[napi(js_name = "resetNativeCounterStats")]
pub fn reset_native_counter_stats() {
    FINALIZED_COUNTERS.store(0, Ordering::Release);
}
