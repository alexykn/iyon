use crate::transcript::row::TranscriptRow;
use crate::tools::{
    registry::ToolRenderer,
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct WriteRenderer;

impl ToolRenderer for WriteRenderer {
    fn tool_name(&self) -> &'static str {
        "write"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("...");
        let mut rows = vec![
            TranscriptRow::blank(),
            TranscriptRow::styled(format!("● write {path} — {}", input.status), input.style),
        ];
        if let Some(content) = input
            .arguments
            .get("content")
            .and_then(serde_json::Value::as_str)
        {
            let preview = truncate_preview_text(content);
            if !preview.is_empty() {
                rows.extend(
                    preview
                        .lines()
                        .map(|line| TranscriptRow::indented(line.to_string(), input.style)),
                );
            }
        }
        rows
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> Vec<TranscriptRow> {
        let title = if input.is_error { "write failed" } else { "write result" };
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

fn truncate_preview_text(text: &str) -> String {
    const MAX_CHARS: usize = 800;
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let mut output: String = text.chars().take(MAX_CHARS).collect();
    output.push('…');
    output
}
