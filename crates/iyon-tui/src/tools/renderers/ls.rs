use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use crate::transcript::row::TranscriptRow;

#[derive(Debug)]
pub(crate) struct LsRenderer;

impl ToolRenderer for LsRenderer {
    fn tool_name(&self) -> &'static str {
        "ls"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        vec![
            TranscriptRow::blank(),
            TranscriptRow::bullet(format!("ls {path} — {}", input.status), input.style),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        let title = if input.is_error {
            "ls failed"
        } else {
            "ls result"
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
