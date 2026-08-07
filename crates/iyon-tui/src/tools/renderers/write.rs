use crate::presentation::View;
use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style, tool_call},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct WriteRenderer;

impl ToolRenderer for WriteRenderer {
    fn tool_name(&self) -> &'static str {
        "write"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> View {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("...");
        let mut children = vec![tool_call(
            format!("write {path} — {}", status_label(input.status)),
            super::tool_style(input.status),
        )];
        if input.show_arg_preview
            && let Some(content) = input
                .arguments
                .get("content")
                .and_then(serde_json::Value::as_str)
        {
            let preview = truncate_preview_text(content);
            if !preview.is_empty() {
                children.extend(result_lines(&preview, super::tool_style(input.status)));
            }
        }
        column(children)
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        let title = if input.is_error() {
            "write failed"
        } else {
            "write result"
        };
        let mut children = vec![super::tool_result_line(
            title,
            result_style(input.is_error()),
        )];
        children.extend(result_lines(input.text, result_style(input.is_error())));
        column(children)
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

fn status_label(status: crate::transcript::ToolTimelineStatus) -> &'static str {
    match status {
        crate::transcript::ToolTimelineStatus::PendingApproval => "waiting for approval",
        crate::transcript::ToolTimelineStatus::Running => "running",
        crate::transcript::ToolTimelineStatus::Approved => "approved",
        crate::transcript::ToolTimelineStatus::Rejected => "rejected",
        crate::transcript::ToolTimelineStatus::Finished => "finished",
        crate::transcript::ToolTimelineStatus::Failed => "failed",
    }
}
