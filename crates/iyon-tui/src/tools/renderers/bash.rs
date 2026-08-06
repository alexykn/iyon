use ratatui::style::{Color, Style};

use crate::transcript::row::TranscriptRow;
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

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
        let command = input
            .arguments
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        vec![
            TranscriptRow::blank(),
            TranscriptRow::styled(format!("● $ {command} — {}", input.status), input.style),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        let title = if input.is_error {
            "bash failed"
        } else {
            "bash result"
        };
        let mut rows = vec![TranscriptRow::indented(title, input.style)];
        rows.extend(
            input
                .text
                .split('\n')
                .map(|line| TranscriptRow::indented(line.to_string(), input.style)),
        );
        if let Some(path) = input
            .details
            .get("fullOutputPath")
            .and_then(serde_json::Value::as_str)
        {
            let style = Style::default().fg(Color::Yellow);
            rows.push(TranscriptRow::indented(format!("[Full output: {path}]"), style));
        }
        rows
    }
}
