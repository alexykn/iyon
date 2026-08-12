//! Root-coordinate projections with explicit source coverage.

use super::validate::validate_projection;
use crate::{StreamOffset, StreamRange};

/// A validated projection of a contiguous interval in one root source space.
///
/// Every non-empty source interval is represented by exactly one or more
/// contiguous [`ProjectionSpan`] values. A span may contain no output values;
/// that is explicit elision, not an uncovered source gap.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection<T> {
    pub(crate) source_base: StreamOffset,
    pub(crate) stable_through: StreamOffset,
    pub(crate) source_end: StreamOffset,
    pub(crate) sealed: bool,
    pub(crate) spans: Vec<ProjectionSpan<T>>,
}

/// One contiguous source interval and the values projected from it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionSpan<T> {
    pub(crate) source: StreamRange,
    pub(crate) values: Vec<T>,
}

/// Validated construction boundary for a [`Projection`].
#[derive(Debug, Clone)]
pub struct ProjectionBuilder<T> {
    source_base: StreamOffset,
    stable_through: StreamOffset,
    source_end: StreamOffset,
    sealed: bool,
    spans: Vec<ProjectionSpan<T>>,
}

impl<T> ProjectionBuilder<T> {
    pub fn new(
        source_base: StreamOffset,
        stable_through: StreamOffset,
        source_end: StreamOffset,
        sealed: bool,
    ) -> Self {
        Self {
            source_base,
            stable_through,
            source_end,
            sealed,
            spans: Vec::new(),
        }
    }

    /// Adds one output value for a source interval.
    pub fn emit(mut self, source: StreamRange, value: T) -> Self {
        self.spans.push(ProjectionSpan {
            source,
            values: vec![value],
        });
        self
    }

    /// Adds any number of output values for a source interval.
    pub fn emit_many(mut self, source: StreamRange, values: impl IntoIterator<Item = T>) -> Self {
        self.spans.push(ProjectionSpan {
            source,
            values: values.into_iter().collect(),
        });
        self
    }

    /// Explicitly accounts for a source interval with no projected values.
    pub fn elide(self, source: StreamRange) -> Self {
        self.emit_many(source, [])
    }

    pub fn finish(self) -> Result<Projection<T>, super::ProjectionValidationError> {
        let projection = Projection {
            source_base: self.source_base,
            stable_through: self.stable_through,
            source_end: self.source_end,
            sealed: self.sealed,
            spans: self.spans,
        };
        validate_projection(&projection)?;
        Ok(projection)
    }
}

impl<T> Projection<T> {
    pub fn source_base(&self) -> StreamOffset {
        self.source_base
    }

    pub fn stable_through(&self) -> StreamOffset {
        self.stable_through
    }

    pub fn source_end(&self) -> StreamOffset {
        self.source_end
    }

    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn spans(&self) -> &[ProjectionSpan<T>] {
        &self.spans
    }

    /// Changes the value type without changing ownership or stability metadata.
    pub fn map<U>(self, mut map: impl FnMut(T) -> U) -> Projection<U> {
        Projection {
            source_base: self.source_base,
            stable_through: self.stable_through,
            source_end: self.source_end,
            sealed: self.sealed,
            spans: self
                .spans
                .into_iter()
                .map(|span| ProjectionSpan {
                    source: span.source,
                    values: span.values.into_iter().map(&mut map).collect(),
                })
                .collect(),
        }
    }
}

impl<T> ProjectionSpan<T> {
    pub fn source(&self) -> StreamRange {
        self.source
    }

    pub fn values(&self) -> &[T] {
        &self.values
    }
}
