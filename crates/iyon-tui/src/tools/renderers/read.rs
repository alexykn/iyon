use crate::transcript::row::TranscriptRow;
use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct ReadRenderer;

impl ToolRenderer for ReadRenderer {
    fn tool_name(&self) -> &'static str {
        "read"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("...");
        let offset = input
            .arguments
            .get("offset")
            .and_then(serde_json::Value::as_u64);
        let limit = input
            .arguments
            .get("limit")
            .and_then(serde_json::Value::as_u64);
        let suffix = match (offset, limit) {
            (Some(offset), Some(limit)) => format!(":{offset}-{}", offset + limit.saturating_sub(1)),
            (Some(offset), None) => format!(":{offset}"),
            _ => String::new(),
        };

        vec![
            TranscriptRow::blank(),
            TranscriptRow::bullet(format!("read {path}{suffix} — {}", input.status),
                input.style,
            ),
        ]
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        let title = if input.is_error { "read failed" } else { "read result" };
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
