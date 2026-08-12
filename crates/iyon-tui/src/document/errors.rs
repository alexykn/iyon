use std::fmt;

use crate::{StreamRange, projection::ProjectionValidationError};

/// Errors raised while constructing generic text semantic values.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextIrError {
    InvalidName,
    InvalidExactLength { text_len: u64, range_len: u64 },
    InvalidSpan,
    InvalidHeadingLevel,
    DuplicateLinkMark,
    NotCharBoundary,
}

impl fmt::Display for TextIrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => write!(
                f,
                "semantic names must be non-empty and contain no whitespace"
            ),
            Self::InvalidExactLength {
                text_len,
                range_len,
            } => {
                write!(
                    f,
                    "exact text length {text_len} does not match source range length {range_len}"
                )
            }
            Self::InvalidSpan => write!(f, "table spans must be non-zero"),
            Self::InvalidHeadingLevel => write!(f, "heading levels must be between 1 and 6"),
            Self::DuplicateLinkMark => write!(f, "an inline may contain at most one link mark"),
            Self::NotCharBoundary => write!(f, "text split is not at a UTF-8 character boundary"),
        }
    }
}

impl std::error::Error for TextIrError {}

/// Errors raised when a [`crate::Projection`] contains invalid text IR.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextProjectionError {
    Projection(ProjectionValidationError),
    RawMustBeSoleValue {
        source: StreamRange,
    },
    RawByteLengthMismatch {
        source: StreamRange,
        text_len: u64,
    },
    NestedRangeOutsideSpan {
        owner: StreamRange,
        range: StreamRange,
    },
    ExactLengthMismatch {
        range: StreamRange,
        text_len: u64,
    },
}

impl fmt::Display for TextProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(error) => error.fmt(f),
            Self::RawMustBeSoleValue { source } => {
                write!(
                    f,
                    "Raw must be the sole value in projection span {source:?}"
                )
            }
            Self::RawByteLengthMismatch { source, text_len } => write!(
                f,
                "Raw text length {text_len} does not match projection span {source:?}"
            ),
            Self::NestedRangeOutsideSpan { owner, range } => write!(
                f,
                "nested source range {range:?} is outside owning projection span {owner:?}"
            ),
            Self::ExactLengthMismatch { range, text_len } => write!(
                f,
                "exact text length {text_len} does not match nested source range {range:?}"
            ),
        }
    }
}

impl std::error::Error for TextProjectionError {}

impl From<ProjectionValidationError> for TextProjectionError {
    fn from(error: ProjectionValidationError) -> Self {
        Self::Projection(error)
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_whitespace)
}

pub(crate) fn validate_name(value: &str) -> Result<(), TextIrError> {
    valid_name(value)
        .then_some(())
        .ok_or(TextIrError::InvalidName)
}
