use crate::transcript::row::TranscriptRow;
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

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
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
            TranscriptRow::blank(),
            TranscriptRow::styled(
                format!("● find {pattern} in {path} — {}", input.status),
                input.style,
            ),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        let title = if input.is_error { "find failed" } else { "find result" };
        let mut rows = vec![TranscriptRow::indented(title, input.style)];
        rows.extend(
            input
                .text
                .split('\n')
                .map(|line| TranscriptRow::indented(line.to_string(), input.style)),
        );
        rows
    }
}
