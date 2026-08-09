use crate::View;
use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style, tool_call},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

#[derive(Debug)]
pub(crate) struct ReadRenderer;

impl ToolRenderer for ReadRenderer {
    fn tool_name(&self) -> &'static str {
        "read"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> View {
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
            (Some(offset), Some(limit)) => {
                format!(":{offset}-{}", offset + limit.saturating_sub(1))
            }
            (Some(offset), None) => format!(":{offset}"),
            _ => String::new(),
        };
        tool_call(
            format!("read {path}{suffix} — {}", status_label(input.status)),
            super::tool_style(input.status),
        )
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        let title = if input.is_error() {
            "read failed"
        } else {
            "read result"
        };
        let mut children = vec![super::tool_result_line(
            title,
            result_style(input.is_error()),
        )];
        children.extend(result_lines(input.text, result_style(input.is_error())));
        column(children)
    }
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
