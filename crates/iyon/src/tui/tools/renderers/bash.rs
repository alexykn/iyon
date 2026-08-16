use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style, tool_call_line, tool_result_line},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use iyon_tui::{ColorSpec, StyleSpec, View};

#[derive(Debug)]
pub(crate) struct BashRenderer;

impl ToolRenderer for BashRenderer {
    fn tool_name(&self) -> &'static str {
        "bash"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> View {
        let command = input
            .arguments
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        tool_call_line(
            format!("$ {command} — {}", status_label(input.status)),
            input,
        )
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        let is_error = input.is_error();
        let title = if is_error {
            "bash failed"
        } else {
            "bash result"
        };
        let mut children = vec![tool_result_line(title, result_style(is_error))];
        children.extend(result_lines(input.text, result_style(is_error)));
        if let Some(path) = input
            .details
            .get("fullOutputPath")
            .and_then(serde_json::Value::as_str)
        {
            let style = StyleSpec::new().foreground(ColorSpec::theme("text.warning"));
            children.push(tool_result_line(format!("[Full output: {path}]"), style));
        }
        column(children)
    }
}

fn status_label(status: crate::transcript::ToolTimelineStatus) -> &'static str {
    match status {
        crate::transcript::ToolTimelineStatus::Preparing => "preparing",
        crate::transcript::ToolTimelineStatus::Prepared => "ready",
        crate::transcript::ToolTimelineStatus::PendingApproval => "waiting for approval",
        crate::transcript::ToolTimelineStatus::Running => "running",
        crate::transcript::ToolTimelineStatus::Approved => "approved",
        crate::transcript::ToolTimelineStatus::Rejected => "rejected",
        crate::transcript::ToolTimelineStatus::Finished => "finished",
        crate::transcript::ToolTimelineStatus::Failed => "failed",
        crate::transcript::ToolTimelineStatus::Cancelled => "cancelled",
    }
}
