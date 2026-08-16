//! The T1 N-API bridge contains only viability probes.
//!
//! This crate is deliberately a bridge, not an application layer. Product
//! behavior stays in TypeScript and the existing Rust libraries.

mod error;
mod sync;
mod tui;

pub use error::NativeError;
pub use sync::{echo_buffer, echo_json, echo_string, native_version};
pub use tui::tui_smoke;
