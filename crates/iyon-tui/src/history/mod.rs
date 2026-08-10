//! Optional ordered semantic history capability.
//!
//! History is deliberately independent from ordinary View-based applications.
//! It owns ordered semantic lifetime, semantic layout, and the private native
//! durability frontier for callers that explicitly use native history. It does
//! not own terminal/backend implementation details or terminal writes.

mod boundary;
mod error;
mod id;
mod layout;
mod model;
mod native;
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
pub(crate) use native::{
    NativeTransferError, NativeTransferOutcome, NativeTransferStatus, transfer_native_prefix,
};
pub(crate) use projection::project;
pub(crate) use stream::{ErasedHistoryStream, HistoryStreamHandle};
pub(crate) use unit::{HistoryUnit, HistoryUnitContent};
