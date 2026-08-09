//! Optional ordered semantic history capability.
//!
//! History is deliberately independent from ordinary View-based applications.
//! It owns unit order and semantic lifetime, but not layout, projection,
//! native history, physical rows, or terminal writes.

mod boundary;
mod error;
mod id;
mod model;
mod stream;
mod unit;

#[cfg(test)]
mod tests;

pub(crate) use boundary::FlowBoundary;
pub(crate) use error::HistoryError;
pub(crate) use id::HistoryUnitId;
pub(crate) use model::History;
pub(crate) use stream::{ErasedHistoryStream, HistoryStreamHandle};
pub(crate) use unit::{HistoryUnit, HistoryUnitContent};
