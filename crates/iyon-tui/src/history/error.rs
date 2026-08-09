//! Invariant-preserving errors for the private History model.

use super::HistoryUnitId;
use crate::stream::StreamModelError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HistoryError {
    OpenStreamMustRemainTail { stream: HistoryUnitId },
    UnitNotFound { unit: HistoryUnitId },
    UnitNotLive { unit: HistoryUnitId },
    UnitNotStream { unit: HistoryUnitId },
    FinalViewContainsComponent { unit: HistoryUnitId },
    StreamTypeMismatch { unit: HistoryUnitId },
    StreamAlreadySealed { unit: HistoryUnitId },
    SealedStreamChanged { unit: HistoryUnitId },
    Stream(StreamModelError),
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "history error: {self:?}")
    }
}

impl std::error::Error for HistoryError {}

impl From<StreamModelError> for HistoryError {
    fn from(error: StreamModelError) -> Self {
        Self::Stream(error)
    }
}
