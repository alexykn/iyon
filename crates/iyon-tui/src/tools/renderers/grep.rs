use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use crate::transcript::row::TranscriptRow;

#[derive(Debug)]
pub(crate) struct GrepRenderer;

impl ToolRenderer for GrepRenderer {
    fn tool_name(&self) -> &'static str {
        "grep"
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
            TranscriptRow::bullet(
                format!("grep /{pattern}/ in {path} — {}", input.status),
                input.style,
            ),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        let title = if input.is_error {
            "grep failed"
        } else {
            "grep result"
        };
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
