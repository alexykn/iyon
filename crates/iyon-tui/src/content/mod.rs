//! Immutable semantic content models and their geometry-independent lowering
//! into [`crate::View`].

mod render;
pub(crate) mod text;

pub use render::Renderer;
