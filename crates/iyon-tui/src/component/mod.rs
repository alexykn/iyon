mod graph;
mod id;
mod mount;
mod registry;
mod revision;
mod slot;
mod tick;

#[cfg(test)]
mod mount_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tick_tests;

use std::fmt::Debug;

use crate::presentation::View;

pub(crate) use graph::{MountGraph, MountNode};
pub(crate) use id::{ComponentHandle, ComponentId};
pub(crate) use mount::{MountTransition, MountTransitions, MountedComponents};
pub(crate) use registry::ComponentRegistry;
pub(crate) use revision::ComponentRevision;
pub(crate) use tick::{TickRegistrationError, TickScheduler};

/// Minimal retained-state rendering contract.
pub(crate) trait Component: Debug + 'static {
    fn view(&self) -> View;
}
