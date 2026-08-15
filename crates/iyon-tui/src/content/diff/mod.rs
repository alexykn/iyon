//! Source-format-independent semantic diff content and rendering.

mod model;
mod render;

pub use model::{
    DiffHunk, DiffLine, DiffLineKind, DiffLineNumber, DiffLineOffset, DiffLineTermination,
    DiffRange, DiffValidationError,
};
pub use render::DiffRenderer;
