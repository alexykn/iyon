//! Private backend-neutral native-history acknowledgement seam.

use crate::physical::PhysicalRow;

/// Receives an exact prefix of final backend-neutral rows.
///
/// `Ok(k)` means exactly `rows[..k]` entered native history and no later row
/// did. An adapter that accepts a prefix and then encounters an error must
/// report that prefix as `Ok(k)`; an `Err` means that this call accepted zero
/// rows.
pub(crate) trait NativeHistorySink {
    type Error;

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error>;
}
