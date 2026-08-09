//! Backend-neutral component interaction vocabulary and routing internals.

mod command;
mod focus;
mod global;
mod key;
mod result;
mod routing;

pub use command::ComponentCx;
pub(crate) use command::{ComponentCapabilities, MountedCapabilities};
pub(crate) use focus::FocusState;
pub(crate) use global::GlobalBindings;
pub use key::{Key, KeyStroke, MediaKey, ModifierKey, Modifiers};
pub use result::InteractionResult;
pub(crate) use routing::{KeyRouter, route_key, route_paste};

#[cfg(test)]
mod tests;
