use std::sync::Arc;

use crate::tools::{
    renderers::GenericRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use iyon_tui::View;

/// FEATURE EXTENSION API.
///
/// Tool-specific visual composition belongs here. Renderers return semantic
/// `View` values and may choose arbitrary generic composition and decoration.
/// They must not perform wrapping, terminal writes, scrollback commits, or
/// physical positioning.
pub(crate) trait ToolRenderer: Send + Sync {
    fn render_call(&self, input: ToolCallRenderInput<'_>) -> View;
    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View;
}

/// FEATURE EXTENSION API. Registry-backed tool renderer selection.
#[derive(Clone)]
pub(crate) struct ToolRendererRegistry {
    generic: Arc<dyn ToolRenderer>,
}

impl std::fmt::Debug for ToolRendererRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRendererRegistry").finish()
    }
}

impl Default for ToolRendererRegistry {
    fn default() -> Self {
        let generic: Arc<dyn ToolRenderer> = Arc::new(GenericRenderer);
        Self { generic }
    }
}

impl ToolRendererRegistry {
    pub(crate) fn render_call(&self, input: ToolCallRenderInput<'_>) -> View {
        self.generic.render_call(input)
    }

    pub(crate) fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        self.generic.render_result(input)
    }
}
