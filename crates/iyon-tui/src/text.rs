//! Generic semantic text IR and front-end/rendering APIs.
//!
//! This layer owns provenance-bearing text values, persistent blocks and
//! inline content, traversal, rewriting, Markdown/plain-text projectors, and
//! the semantic-to-`View` renderer. It intentionally does not own History,
//! streaming lifecycle, terminal geometry, or application state.

pub use crate::content::text::*;
