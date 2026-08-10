use crate::transcript::ToolTimelineStatus;

/// FEATURE EXTENSION API.
///
/// Semantic tool-call state supplied to a tool renderer. It intentionally has
/// no Ratatui style or physical row information.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolCallRenderInput<'a> {
    pub(crate) tool_name: &'a str,
    pub(crate) arguments: &'a serde_json::Value,
    pub(crate) status: ToolTimelineStatus,
    pub(crate) show_arg_preview: bool,
}

/// FEATURE EXTENSION API.
///
/// Semantic tool-result state supplied to a tool renderer. Renderers choose
/// their visual decoration; the compiler owns wrapping and clipping.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolResultRenderInput<'a> {
    pub(crate) tool_name: &'a str,
    pub(crate) text: &'a str,
    pub(crate) details: &'a serde_json::Value,
    pub(crate) outcome: ToolOutcome,
}

/// FEATURE EXTENSION API. Generic outcome vocabulary for tool presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolOutcome {
    Success,
    Error,
}

impl ToolResultRenderInput<'_> {
    pub(crate) fn is_error(self) -> bool {
        self.outcome == ToolOutcome::Error
    }
}
