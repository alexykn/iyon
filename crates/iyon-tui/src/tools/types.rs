use ratatui::style::Style;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolCallRenderInput<'a> {
    pub(crate) tool_name: &'a str,
    pub(crate) arguments: &'a serde_json::Value,
    pub(crate) status: &'a str,
    pub(crate) style: Style,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ToolResultRenderInput<'a> {
    pub(crate) tool_name: &'a str,
    pub(crate) text: &'a str,
    pub(crate) details: &'a serde_json::Value,
    pub(crate) is_error: bool,
    pub(crate) style: Style,
}
