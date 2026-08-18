//! Shared semantic Stream row indexing and viewport slicing.

mod index;
mod window;

pub(crate) use index::{StreamRowIndex, build_index, build_index_from, reindex_from};

#[cfg(test)]
pub(crate) use index::{build_index_call_count, reset_build_index_call_count};
pub(crate) use window::window_view;
