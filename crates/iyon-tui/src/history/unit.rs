//! Ordered semantic History unit representation.

use crate::presentation::View;

use super::{ErasedHistoryStream, FlowBoundary, HistoryUnitId};

pub(crate) struct HistoryUnit {
    pub(super) id: HistoryUnitId,
    pub(super) boundary: FlowBoundary,
    pub(super) content: HistoryUnitContent,
}

pub(crate) enum HistoryUnitContent {
    Static(View),
    Live(View),
    Stream(ErasedHistoryStream),
}
