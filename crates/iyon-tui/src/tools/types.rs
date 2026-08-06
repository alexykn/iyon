use ratatui::style::Style;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolCallRenderInput<'a> {
    pub(crate) tool_name: &'a str,
    pub(crate) arguments: &'a serde_json::Value,
    pub(crate) status: &'a str,
    pub(crate) style: Style,
    /// When true, renderers may show a raw JSON/argument preview underneath the
    /// call header (a debug aid). Default off: calls render as a compact header
    /// only, and the meaningful payload lives in the tool result.
    pub(crate) show_arg_preview: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolResultRenderInput<'a> {
    pub(crate) tool_name: &'a str,
    pub(crate) text: &'a str,
    pub(crate) details: &'a serde_json::Value,
    pub(crate) is_error: bool,
    pub(crate) style: Style,
}
