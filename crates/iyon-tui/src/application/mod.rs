//! Generic standalone application kernel.
//!
//! [`App`] owns application state and framework runtime state while [`AppCx`]
//! exposes the narrow capabilities available to `init` and `update`.
//! Components continue to handle local interaction; their typed outputs are
//! routed into the application's action queue.

mod app;
mod context;
mod kernel;
mod timer;

#[cfg(test)]
mod tests;

pub use app::App;
pub use context::AppCx;
pub use timer::TimerHandle;
