use std::{collections::HashMap, sync::Arc};

use crate::tools::{
    renderers::{
        BashRenderer, EditRenderer, FindRenderer, GenericRenderer, GrepRenderer, LsRenderer,
        ReadRenderer, WriteRenderer,
    },
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use crate::transcript::row::TranscriptRow;

/// A tool renderer turns a call (or its result) into transcript rows. Rows carry
/// their intended hanging indent as a first-class `TranscriptRow`; renderers must
/// NOT bake leading spaces into span content (that breaks wrapped continuation
/// lines and steals wrap width).
pub(crate) trait ToolRenderer: Send + Sync {
    fn tool_name(&self) -> &'static str;
    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow>;
    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow>;
}

#[derive(Clone)]
pub(crate) struct ToolRendererRegistry {
    generic: Arc<dyn ToolRenderer>,
    builtins: HashMap<&'static str, Arc<dyn ToolRenderer>>,
}

impl std::fmt::Debug for ToolRendererRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRendererRegistry")
            .field("builtin_count", &self.builtins.len())
            .finish()
    }
}

impl Default for ToolRendererRegistry {
    fn default() -> Self {
        let generic: Arc<dyn ToolRenderer> = Arc::new(GenericRenderer);
        let mut builtins: HashMap<&'static str, Arc<dyn ToolRenderer>> = HashMap::new();

        for renderer in [
            Arc::new(BashRenderer) as Arc<dyn ToolRenderer>,
            Arc::new(EditRenderer),
            Arc::new(ReadRenderer),
            Arc::new(WriteRenderer),
            Arc::new(GrepRenderer),
            Arc::new(FindRenderer),
            Arc::new(LsRenderer),
        ] {
            builtins.insert(renderer.tool_name(), renderer);
        }

        Self { generic, builtins }
    }
}

impl ToolRendererRegistry {
    pub(crate) fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
        self.builtins
            .get(input.tool_name)
            .unwrap_or(&self.generic)
            .render_call(input)
    }

    pub(crate) fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        self.builtins
            .get(input.tool_name)
            .unwrap_or(&self.generic)
            .render_result(input)
    }
}
