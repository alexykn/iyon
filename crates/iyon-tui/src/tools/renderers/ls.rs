use ratatui::text::Line;

use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct LsRenderer;

impl ToolRenderer for LsRenderer {
    fn tool_name(&self) -> &'static str {
        "ls"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<Line<'static>> {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        vec![
            Line::from(""),
            Line::styled(format!("● ls {path} — {}", input.status), input.style),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<Line<'static>> {
        let title = if input.is_error { "ls failed" } else { "ls result" };
        let mut rows = vec![Line::styled(format!("  {title}"), input.style)];
        rows.extend(input.text.split('\n').map(|line| Line::styled(format!("  {line}"), input.style)));
        rows
    }
}
