//! Validation for projection values, relationships, and transitions.

use std::fmt;

use crate::StreamOffset;

use super::value::{Projection, ProjectionSpan};

/// Construction failures for a projection.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionValidationError {
    InvalidFrontier,
    EmptySpan,
    FirstSpanDoesNotStartAtBase,
    GapOrOverlap,
    SpanBeyondSourceEnd,
    TrailingUncoveredSource,
    StableFrontierInsideSpan,
    SealedBeforeStableEnd,
}

/// Failures when comparing successive snapshots of one projection stage.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionTransitionError {
    SourceBaseRegressed,
    SourceBaseBeyondPreviousStability,
    SourceEndRegressed,
    StabilityRegressed,
    StablePrefixChanged,
    UnsealedAfterSeal,
    ChangedAfterSeal,
}

/// Failures when checking an output projection against its input projection.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionRelationError {
    SourceBaseMismatch,
    OutputEndBeyondInput,
    OutputStabilityBeyondInput,
    OutputSealedBeforeInput,
    SealedOutputNotCaughtUp,
}

impl fmt::Display for ProjectionValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid projection: {self:?}")
    }
}

impl std::error::Error for ProjectionValidationError {}

impl fmt::Display for ProjectionTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid projection transition: {self:?}")
    }
}

impl std::error::Error for ProjectionTransitionError {}

impl fmt::Display for ProjectionRelationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid projection relation: {self:?}")
    }
}

impl std::error::Error for ProjectionRelationError {}

pub(crate) fn validate_projection<T>(
    projection: &Projection<T>,
) -> Result<(), ProjectionValidationError> {
    if projection.source_base > projection.stable_through
        || projection.stable_through > projection.source_end
    {
        return Err(ProjectionValidationError::InvalidFrontier);
    }
    if projection.sealed && projection.stable_through != projection.source_end {
        return Err(ProjectionValidationError::SealedBeforeStableEnd);
    }

    if projection.source_base == projection.source_end {
        if projection.spans.is_empty() {
            return Ok(());
        }
        return Err(ProjectionValidationError::SpanBeyondSourceEnd);
    }

    let mut expected = projection.source_base;
    for (index, span) in projection.spans.iter().enumerate() {
        if span.source.start() >= span.source.end() {
            return Err(ProjectionValidationError::EmptySpan);
        }
        if span.source.end() > projection.source_end {
            return Err(ProjectionValidationError::SpanBeyondSourceEnd);
        }
        if index == 0 && span.source.start() != projection.source_base {
            return Err(ProjectionValidationError::FirstSpanDoesNotStartAtBase);
        }
        if span.source.start() != expected {
            return Err(ProjectionValidationError::GapOrOverlap);
        }
        expected = span.source.end();
    }
    if expected != projection.source_end {
        return Err(ProjectionValidationError::TrailingUncoveredSource);
    }
    if projection.stable_through != projection.source_base
        && projection.stable_through != projection.source_end
        && !projection
            .spans
            .iter()
            .any(|span| span.source.end() == projection.stable_through)
    {
        return Err(ProjectionValidationError::StableFrontierInsideSpan);
    }
    Ok(())
}

/// Validates that an output stage accounts for a prefix of its input stage.
pub fn validate_projection_relation<I, O>(
    input: &Projection<I>,
    output: &Projection<O>,
) -> Result<(), ProjectionRelationError> {
    if output.source_base != input.source_base {
        return Err(ProjectionRelationError::SourceBaseMismatch);
    }
    if output.source_end > input.source_end {
        return Err(ProjectionRelationError::OutputEndBeyondInput);
    }
    if output.stable_through > input.stable_through {
        return Err(ProjectionRelationError::OutputStabilityBeyondInput);
    }
    if output.sealed && !input.sealed {
        return Err(ProjectionRelationError::OutputSealedBeforeInput);
    }
    if output.sealed && output.source_end != input.source_end {
        return Err(ProjectionRelationError::SealedOutputNotCaughtUp);
    }
    Ok(())
}

/// Validates the monotonic transition between two snapshots of one stage.
pub fn validate_projection_transition<T: PartialEq>(
    previous: &Projection<T>,
    next: &Projection<T>,
) -> Result<(), ProjectionTransitionError> {
    if previous.sealed {
        if !next.sealed {
            return Err(ProjectionTransitionError::UnsealedAfterSeal);
        }
        if previous != next {
            return Err(ProjectionTransitionError::ChangedAfterSeal);
        }
        return Ok(());
    }
    if next.source_base < previous.source_base {
        return Err(ProjectionTransitionError::SourceBaseRegressed);
    }
    if next.source_base > previous.stable_through {
        return Err(ProjectionTransitionError::SourceBaseBeyondPreviousStability);
    }
    if next.source_end < previous.source_end {
        return Err(ProjectionTransitionError::SourceEndRegressed);
    }
    if next.stable_through < previous.stable_through {
        return Err(ProjectionTransitionError::StabilityRegressed);
    }
    if next.sealed && next.stable_through != next.source_end {
        return Err(ProjectionTransitionError::ChangedAfterSeal);
    }

    let overlap_end = previous.stable_through;
    if overlap_end > next.source_base
        && !same_stable_projection(&previous.spans, &next.spans, next.source_base, overlap_end)
    {
        return Err(ProjectionTransitionError::StablePrefixChanged);
    }
    Ok(())
}

fn same_stable_projection<T: PartialEq>(
    previous: &[ProjectionSpan<T>],
    next: &[ProjectionSpan<T>],
    start: StreamOffset,
    end: StreamOffset,
) -> bool {
    let Some(mut previous_index) = first_span_at_or_after(previous, start) else {
        return false;
    };
    let Some(mut next_index) = first_span_at_or_after(next, start) else {
        return false;
    };
    let mut cursor = start;

    while cursor < end {
        let Some(previous_span) = previous.get(previous_index) else {
            return false;
        };
        let Some(next_span) = next.get(next_index) else {
            return false;
        };
        if previous_span.source.start() > cursor
            || next_span.source.start() > cursor
            || previous_span.source.end() <= cursor
            || next_span.source.end() <= cursor
        {
            return false;
        }

        // Clipping the first span permits compaction inside a span. Every
        // subsequent end must match, preserving stable segmentation.
        let previous_end = previous_span.source.end().min(end);
        let next_end = next_span.source.end().min(end);
        if previous_end != next_end || previous_span.values != next_span.values {
            return false;
        }

        cursor = previous_end;
        previous_index += 1;
        next_index += 1;
    }

    cursor == end
}

fn first_span_at_or_after<T>(spans: &[ProjectionSpan<T>], offset: StreamOffset) -> Option<usize> {
    let index = spans.partition_point(|span| span.source.end() <= offset);
    (index < spans.len()).then_some(index)
}
