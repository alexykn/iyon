//! Shared semantic Stream row indexing and viewport slicing.

mod index;
mod window;

pub(crate) use index::{StreamRowIndex, build_index};
pub(crate) use window::window_view;
