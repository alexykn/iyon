//! Generic semantic text to [`View`] lowering.
//!
//! The renderer emits reserved text [`StyleRef`] identity plus typed
//! [`TextFacts`]. Semantic paint is resolved later by Theme.

mod block;
mod identity;
mod inline;
mod policy;

#[cfg(test)]
mod tests;

pub use policy::{SoftBreakPolicy, TextRenderPolicy};

use super::{Block, TextContent, text_style_ref};
use crate::{IntoView, View};
use identity::RenderContext;

/// Converts a semantic value into the generic presentation [`View`].
///
/// Renderers do not receive terminal geometry, parser state, clocks, or
/// stream lifecycle. Width-dependent layout remains in the View pipeline.
pub trait Renderer<Input: ?Sized> {
    fn render(&self, input: &Input) -> View;
}

/// The one generic renderer for the frozen text IR.
#[derive(Clone, Debug, Default)]
pub struct TextRenderer {
    policy: TextRenderPolicy,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_policy(policy: TextRenderPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> &TextRenderPolicy {
        &self.policy
    }

    pub fn render_block(&self, block: &Block) -> View {
        self.lower_block(block, &RenderContext::default())
    }
}

impl Renderer<TextContent> for TextRenderer {
    fn render(&self, input: &TextContent) -> View {
        match input {
            TextContent::Raw(raw) => View::text(raw.text()).style(text_style_ref()).into_view(),
            TextContent::Block(block) => self.lower_block(block, &RenderContext::default()),
        }
    }
}

impl Renderer<Block> for TextRenderer {
    fn render(&self, input: &Block) -> View {
        self.lower_block(input, &RenderContext::default())
    }
}

impl Renderer<[TextContent]> for TextRenderer {
    fn render(&self, input: &[TextContent]) -> View {
        View::vertical(|column| {
            column.gap(self.policy.block_gap());
            for content in input {
                column.child(Renderer::render(self, content));
            }
        })
    }
}
