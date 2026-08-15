pub(crate) mod registry;
pub(crate) mod renderers;
pub(crate) mod types;
pub(crate) mod unified_diff;

pub(crate) use registry::ToolRendererRegistry;
pub(crate) use types::{ToolCallRenderInput, ToolOutcome, ToolResultRenderInput};

pub(crate) const MAX_COLLAPSED_TOOL_ROWS: u16 = 16;
