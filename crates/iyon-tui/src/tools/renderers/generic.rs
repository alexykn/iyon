use crate::transcript::row::TranscriptRow;
use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct GenericRenderer;

impl ToolRenderer for GenericRenderer {
    fn tool_name(&self) -> &'static str {
        "*"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
        let mut rows = vec![
            TranscriptRow::blank(),
            TranscriptRow::bullet(format!("tool {} — {}", input.tool_name, input.status),
                input.style,
            ),
        ];
        let preview = serde_json::to_string_pretty(input.arguments)
            .unwrap_or_else(|_| input.arguments.to_string());
        if !preview.is_empty() {
            rows.extend(
                preview
                    .lines()
                    .map(|line| TranscriptRow::indented(line.to_string(), input.style)),
            );
        }
        rows
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        let title = if input.is_error { "failed" } else { "result" };
        let mut rows = vec![TranscriptRow::indented(
            format!("{} {title}", input.tool_name),
            input.style,
        )];
        rows.extend(
            input
                .text
                .split('\n')
                .map(|line| TranscriptRow::indented(line.to_string(), input.style)),
        );
        rows
    }
}
