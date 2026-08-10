use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style, tool_call},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use iyon_tui::View;

#[derive(Debug)]
pub(crate) struct FindRenderer;

impl ToolRenderer for FindRenderer {
    fn tool_name(&self) -> &'static str {
        "find"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> View {
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
        tool_call(
            format!("find {pattern} in {path} — {}", status_label(input.status)),
            super::tool_style(input.status),
        )
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        let title = if input.is_error() {
            "find failed"
        } else {
            "find result"
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
