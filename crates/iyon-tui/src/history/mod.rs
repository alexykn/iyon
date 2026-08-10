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

pub use boundary::FlowBoundary;
pub use error::HistoryError;
pub use id::HistoryUnitId;
pub use layout::HistoryLayout;
pub use model::History;
pub(crate) use native::{NativeTransferError, NativeTransferStatus, transfer_native_prefix};
pub(crate) use projection::{HistoryPhysicalOverlay, project_into_session_for_host};
pub(crate) use stream::ErasedHistoryStream;
pub use stream::HistoryStreamHandle;
pub(crate) use unit::{HistoryUnit, HistoryUnitContent};
