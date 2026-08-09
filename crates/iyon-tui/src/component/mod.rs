mod id;
mod registry;

#[cfg(test)]
mod tests;

use std::fmt::Debug;

use crate::presentation::View;

pub(crate) use id::{ComponentHandle, ComponentId};
pub(crate) use registry::ComponentRegistry;

/// Minimal retained-state rendering contract.
pub(crate) trait Component: Debug + 'static {
    fn view(&self) -> View;
}
