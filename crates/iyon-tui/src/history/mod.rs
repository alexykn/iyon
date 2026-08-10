//! Optional ordered semantic history capability.
//!
//! History is deliberately independent from ordinary View-based applications.
//! It owns unit order, semantic lifetime, and semantic layout settings, but
//! not projection viewport state, native history, physical rows, or terminal
//! writes.

mod boundary;
mod error;
mod id;
mod layout;
mod model;
mod projection;
mod stream;
mod unit;

#[cfg(test)]
mod tests;

pub(crate) use boundary::FlowBoundary;
pub(crate) use error::HistoryError;
pub(crate) use id::HistoryUnitId;
pub(crate) use layout::HistoryLayout;
pub(crate) use model::History;
pub(crate) use projection::project;
pub(crate) use stream::{ErasedHistoryStream, HistoryStreamHandle};
pub(crate) use unit::{HistoryUnit, HistoryUnitContent};
