//! Typed semantic output channels and deferred event routing.
//!
//! This module owns the generic, backend-neutral boundary between component
//! event emission and application action mapping. It intentionally does not
//! know about components, views, input, or runtime state.

mod event;
mod handle;
mod router;

#[cfg(test)]
mod tests;

pub(crate) use event::{EventCx, OutputQueue};
pub(crate) use handle::Output;
pub(crate) use router::{OutputDispatchError, OutputRouter, RouteConflict};
