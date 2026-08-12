//! Static composition of projection stages.

use super::{Projection, ProjectionRelationError, Projector, validate_projection_relation};

/// A statically typed two-stage projector composition.
pub struct Then<A, B> {
    pub(crate) first: A,
    pub(crate) second: B,
}

impl<A, B> Then<A, B> {
    pub(crate) const fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

/// Errors from either projector or either stage's relation contract.
#[non_exhaustive]
pub enum ThenError<A, B> {
    First(A),
    FirstRelation(ProjectionRelationError),
    Second(B),
    SecondRelation(ProjectionRelationError),
}

impl<A, B> std::fmt::Debug for ThenError<A, B>
where
    A: std::fmt::Debug,
    B: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::First(error) => f.debug_tuple("First").field(error).finish(),
            Self::FirstRelation(error) => f.debug_tuple("FirstRelation").field(error).finish(),
            Self::Second(error) => f.debug_tuple("Second").field(error).finish(),
            Self::SecondRelation(error) => f.debug_tuple("SecondRelation").field(error).finish(),
        }
    }
}

impl<A, B> std::fmt::Display for ThenError<A, B>
where
    A: std::fmt::Display,
    B: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::First(error) => write!(f, "first projector failed: {error}"),
            Self::FirstRelation(error) => write!(f, "first projector contract failed: {error}"),
            Self::Second(error) => write!(f, "second projector failed: {error}"),
            Self::SecondRelation(error) => write!(f, "second projector contract failed: {error}"),
        }
    }
}

impl<A, B> std::error::Error for ThenError<A, B>
where
    A: std::error::Error,
    B: std::error::Error,
{
}

impl<Input, A, B> Projector<Input> for Then<A, B>
where
    A: Projector<Input>,
    B: Projector<A::Output>,
{
    type Output = B::Output;
    type Error = ThenError<A::Error, B::Error>;

    fn project(
        &mut self,
        input: &Projection<Input>,
    ) -> Result<Projection<Self::Output>, Self::Error> {
        let middle = self.first.project(input).map_err(ThenError::First)?;
        validate_projection_relation(input, &middle).map_err(ThenError::FirstRelation)?;
        let output = self.second.project(&middle).map_err(ThenError::Second)?;
        validate_projection_relation(&middle, &output).map_err(ThenError::SecondRelation)?;
        Ok(output)
    }

    fn restart_from(&self, output_from: crate::StreamOffset) -> crate::StreamOffset {
        let second = self.second.restart_from(output_from);
        debug_assert!(second <= output_from);
        let first = self.first.restart_from(second);
        debug_assert!(first <= second);
        first
    }
}
