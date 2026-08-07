use crate::presentation::{ColorSpec, StyleSpec, View};
use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style, tool_call, tool_result_line, tool_style},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};

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
        tool_call(
            format!("$ {command} — {}", status_label(input.status)),
            tool_style(input.status),
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
            let style = StyleSpec {
                foreground: Some(ColorSpec::Theme("text.warning".into())),
                ..StyleSpec::default()
            };
            children.push(tool_result_line(format!("[Full output: {path}]"), style));
        }
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
