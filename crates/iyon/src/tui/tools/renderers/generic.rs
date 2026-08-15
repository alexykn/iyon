use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style, tool_call_line, tool_result_line, tool_style},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use iyon_tui::View;

#[derive(Debug)]
pub(crate) struct GenericRenderer;

impl ToolRenderer for GenericRenderer {
    fn tool_name(&self) -> &'static str {
        "*"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> View {
        let mut children = vec![tool_call_line(
            format!("tool {} — {}", input.tool_name, status_label(input.status)),
            input,
        )];
        if input.show_arg_preview {
            let preview = serde_json::to_string_pretty(input.arguments)
                .unwrap_or_else(|_| input.arguments.to_string());
            children.extend(result_lines(&preview, tool_style(input.status)));
        }
        column(children)
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        let is_error = input.is_error();
        let title = if is_error { "failed" } else { "result" };
        let mut children = vec![tool_result_line(
            format!("{} {title}", input.tool_name),
            result_style(is_error),
        )];
        children.extend(result_lines(input.text, result_style(is_error)));
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
