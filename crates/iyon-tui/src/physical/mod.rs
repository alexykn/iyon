//! Backend-neutral physical presentation vocabulary.
//!
//! This layer contains resolved cell styles, mutable composition surfaces, and
//! immutable rows. It has no knowledge of semantic views, streams, themes, or
//! terminal backends.

mod row;
mod style;
mod surface;

#[cfg(test)]
mod tests;

pub(crate) use row::PhysicalRow;
pub(crate) use style::{PhysicalColor, PhysicalStyle};
pub(crate) use surface::{PhysicalCell, Surface};
