use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style, text, tool_call, tool_result_line},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use iyon_tui::{ColorSpec, IntoView, StyleSpec, View};

#[derive(Debug)]
pub(crate) struct EditRenderer;

impl ToolRenderer for EditRenderer {
    fn tool_name(&self) -> &'static str {
        "edit"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> View {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("...");
        let mut children = vec![tool_call(
            format!("edit {path} — {}", status_label(input.status)),
            super::tool_style(input.status),
        )];
        // Debug-only: surface the raw edits payload; the normal view keeps the
        // call compact and lets the result's diff carry the detail.
        if input.show_arg_preview
            && let Some(edits) = input.arguments.get("edits")
        {
            let preview = serde_json::to_string_pretty(edits).unwrap_or_else(|_| edits.to_string());
            children.extend(result_lines(&preview, super::tool_style(input.status)));
        }
        column(children)
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        if input.is_error() {
            let mut children = vec![tool_result_line("edit failed", result_style(true))];
            children.extend(result_lines(input.text, result_style(true)));
            return column(children);
        }

        let mut children = vec![tool_result_line(input.text, result_style(false))];
        if let Some(diff) = input
            .details
            .get("diff")
            .and_then(serde_json::Value::as_str)
        {
            children.extend(diff.lines().map(format_diff_line));
        }
        column(children)
    }
}

fn format_diff_line(line: &str) -> View {
    let color = if line.starts_with('+') && !line.starts_with("+++") {
        ColorSpec::ansi(2)
    } else if line.starts_with('-') && !line.starts_with("---") {
        ColorSpec::ansi(1)
    } else if line.starts_with("@@") {
        ColorSpec::ansi(3)
    } else {
        ColorSpec::theme("text.muted")
    };
    text(line, StyleSpec::new().foreground(color)).into_view()
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
