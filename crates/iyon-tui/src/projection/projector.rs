//! Generic stateful projection stages.

use super::value::Projection;

/// Transforms one root-coordinate projection into another projection.
///
/// The trait intentionally has no thread-safety or storage bounds. A
/// projector may retain local state as needed.
pub trait Projector<Input> {
    type Output;
    type Error;

    fn project(
        &mut self,
        input: &Projection<Input>,
    ) -> Result<Projection<Self::Output>, Self::Error>;

    /// Returns the earliest root coordinate needed to reconstruct output from
    /// `output_from` using the projector's currently retained state.
    ///
    /// The result is conservative and remains in the shared root coordinate
    /// space. Implementors must always return `restart_from(X) <= X`.
    fn restart_from(&self, output_from: crate::StreamOffset) -> crate::StreamOffset {
        output_from
    }
}

/// Extension methods for statically composing projectors.
pub trait ProjectorExt<Input>: Projector<Input> + Sized {
    fn then<P>(self, next: P) -> super::Then<Self, P>
    where
        P: Projector<Self::Output>,
    {
        super::Then::new(self, next)
    }
}

impl<Input, P> ProjectorExt<Input> for P where P: Projector<Input> + Sized {}
