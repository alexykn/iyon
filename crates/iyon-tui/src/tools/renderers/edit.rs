use ratatui::{style::{Color, Style}, text::Line};

use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct EditRenderer;

impl ToolRenderer for EditRenderer {
    fn tool_name(&self) -> &'static str {
        "edit"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<Line<'static>> {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("...");
        let mut rows = vec![
            Line::from(""),
            Line::styled(format!("● edit {path} — {}", input.status), input.style),
        ];
        if let Some(edits) = input.arguments.get("edits") {
            let preview = serde_json::to_string_pretty(edits).unwrap_or_else(|_| edits.to_string());
            rows.extend(
                preview
                    .lines()
                    .map(|line| Line::styled(format!("  {line}"), input.style)),
            );
        }
        rows
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<Line<'static>> {
        if input.is_error {
            let mut rows = vec![Line::styled("  edit failed", input.style)];
            rows.extend(input.text.split('\n').map(|line| Line::styled(format!("  {line}"), input.style)));
            return rows;
        }

        let mut rows = vec![Line::styled(format!("  {}", input.text), input.style)];
        if let Some(diff) = input.details.get("diff").and_then(serde_json::Value::as_str) {
            rows.extend(diff.lines().map(format_diff_line));
        }
        rows
    }
}

fn format_diff_line(line: &str) -> Line<'static> {
    let style = if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Rgb(113, 128, 150))
    };
    Line::styled(format!("  {line}"), style)
}
