//! Backend-neutral component interaction vocabulary and routing internals.

mod command;
mod focus;
#[cfg(test)]
mod global;
mod key;
mod result;
mod routing;

pub use command::ComponentCx;
pub(crate) use command::{ComponentCapabilities, MountedCapabilities};
pub(crate) use focus::FocusState;
#[cfg(test)]
pub(crate) use global::GlobalBindings;
pub use key::{Key, KeyStroke, MediaKey, ModifierKey, Modifiers};
pub use result::InteractionResult;
pub(crate) use routing::{KeyRouter, route_paste, route_paste_interceptor};

#[cfg(test)]
pub(crate) use routing::route_key;

#[cfg(test)]
mod tests;
