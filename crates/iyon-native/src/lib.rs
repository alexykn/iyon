//! The T1 N-API bridge contains only viability probes.
//!
//! This crate is deliberately a bridge, not an application layer. Product
//! behavior stays in TypeScript and the existing Rust libraries.

mod api;
mod async_ops;
mod core;
mod credentials;
mod error;
mod events;
mod handles;
mod model_turn;
mod queue;
mod sync;
mod tool_execution;
mod tui;
mod value;

pub use async_ops::async_sleep;
pub use core::KernelSession;
pub use credentials::{credential_delete, credential_get, credential_has, credential_set};
pub use error::NativeError;
pub use handles::{
    CancellationProbe, NativeCounter, native_counter_stats, reset_native_counter_stats,
};
pub use model_turn::ModelTurn;
pub use queue::EventQueueProbe;
pub use sync::{echo_buffer, echo_json, echo_string, native_version};
pub use tool_execution::ToolExecution;
pub use tui::tui_smoke;
