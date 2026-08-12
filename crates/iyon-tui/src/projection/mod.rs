//! Generic, root-coordinate semantic projection algebra.
//!
//! This module deliberately stops before terminal streaming, layout, and
//! presentation. It is suitable for text, records, logs, or event streams.

mod compose;
mod projector;
mod smooth;
mod validate;
mod value;

#[cfg(test)]
mod tests;

pub use compose::{Then, ThenError};
pub use projector::{Projector, ProjectorExt};
pub use smooth::{Smooth, SmoothConfig, SmoothConfigError};
pub(crate) use validate::validate_projection;
pub use validate::{
    ProjectionRelationError, ProjectionTransitionError, ProjectionValidationError,
    validate_projection_relation, validate_projection_transition,
};
pub use value::{Projection, ProjectionBuilder, ProjectionSpan};
