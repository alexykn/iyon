//! Invariant-preserving errors for the public History model.

use super::HistoryUnitId;
use crate::stream::StreamError;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    OpenStreamMustRemainTail { stream: HistoryUnitId },
    UnitNotFound { unit: HistoryUnitId },
    UnitNotLive { unit: HistoryUnitId },
    UnitNotStream { unit: HistoryUnitId },
    FinalViewContainsComponent { unit: HistoryUnitId },
    StreamTypeMismatch { unit: HistoryUnitId },
    StreamAlreadySealed { unit: HistoryUnitId },
    SealedStreamChanged { unit: HistoryUnitId },
    Stream(StreamError),
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenStreamMustRemainTail { stream } => {
                write!(
                    formatter,
                    "open stream {stream:?} must remain the History tail"
                )
            }
            Self::UnitNotFound { unit } => write!(formatter, "History unit {unit:?} was not found"),
            Self::UnitNotLive { unit } => write!(formatter, "History unit {unit:?} is not live"),
            Self::UnitNotStream { unit } => {
                write!(formatter, "History unit {unit:?} is not a stream")
            }
            Self::FinalViewContainsComponent { unit } => write!(
                formatter,
                "final view for History unit {unit:?} contains a component"
            ),
            Self::StreamTypeMismatch { unit } => {
                write!(
                    formatter,
                    "stream type does not match History unit {unit:?}"
                )
            }
            Self::StreamAlreadySealed { unit } => {
                write!(formatter, "History stream {unit:?} is already sealed")
            }
            Self::SealedStreamChanged { unit } => {
                write!(formatter, "sealed History stream {unit:?} changed")
            }
            Self::Stream(error) => write!(formatter, "stream error: {error}"),
        }
    }
}

impl std::error::Error for HistoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Stream(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StreamError> for HistoryError {
    fn from(error: StreamError) -> Self {
        Self::Stream(error)
    }
}
