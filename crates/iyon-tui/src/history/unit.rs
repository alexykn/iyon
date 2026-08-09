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

impl HistoryUnitContent {
    pub(super) fn kind_name(&self) -> &'static str {
        match self {
            Self::Static(_) => "Static",
            Self::Live(_) => "Live",
            Self::Stream(_) => "Stream",
        }
    }
}
