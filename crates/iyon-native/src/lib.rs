//! The T1 N-API bridge contains only viability probes.
//!
//! This crate is deliberately a bridge, not an application layer. Product
//! behavior stays in TypeScript and the existing Rust libraries.

mod async_ops;
mod error;
mod handles;
mod queue;
mod sync;
mod tui;

pub use async_ops::async_sleep;
pub use error::NativeError;
pub use handles::{
    CancellationProbe, NativeCounter, native_counter_stats, reset_native_counter_stats,
};
pub use queue::EventQueueProbe;
pub use sync::{echo_buffer, echo_json, echo_string, native_version};
pub use tui::tui_smoke;
