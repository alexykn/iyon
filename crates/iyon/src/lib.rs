//! Deprecated Rust compatibility library. The shipped `iyon` product is owned by
//! the TypeScript/Bun CLI; these exports remain for downstream compatibility and
//! the public-app regression oracle.

pub mod tui;

pub(crate) use tui::tools;
pub(crate) use tui::transcript;
