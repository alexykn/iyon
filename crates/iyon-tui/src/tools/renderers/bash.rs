use ratatui::{style::{Color, Style}, text::Line};

use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct BashRenderer;

impl ToolRenderer for BashRenderer {
    fn tool_name(&self) -> &'static str {
        "bash"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<Line<'static>> {
        let command = input
            .arguments
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        vec![
            Line::from(""),
            Line::styled(format!("● $ {command} — {}", input.status), input.style),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<Line<'static>> {
        let title = if input.is_error {
            "bash failed"
        } else {
            "bash result"
        };
        let mut rows = vec![Line::styled(format!("  {title}"), input.style)];
        rows.extend(
            input
                .text
                .split('\n')
                .map(|line| Line::styled(format!("  {line}"), input.style)),
        );
        if let Some(path) = input
            .details
            .get("fullOutputPath")
            .and_then(serde_json::Value::as_str)
        {
            rows.push(Line::styled(
                format!("  [Full output: {path}]"),
                Style::default().fg(Color::Yellow),
            ));
        }
        rows
    }
}
