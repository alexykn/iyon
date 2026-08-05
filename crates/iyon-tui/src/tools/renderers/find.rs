use ratatui::text::Line;

use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct FindRenderer;

impl ToolRenderer for FindRenderer {
    fn tool_name(&self) -> &'static str {
        "find"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<Line<'static>> {
        let pattern = input
            .arguments
            .get("pattern")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        vec![
            Line::from(""),
            Line::styled(
                format!("● find {pattern} in {path} — {}", input.status),
                input.style,
            ),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<Line<'static>> {
        let title = if input.is_error { "find failed" } else { "find result" };
        let mut rows = vec![Line::styled(format!("  {title}"), input.style)];
        rows.extend(input.text.split('\n').map(|line| Line::styled(format!("  {line}"), input.style)));
        rows
    }
}
