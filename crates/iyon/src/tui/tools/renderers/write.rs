use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_style},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use iyon_tui::{IntoView, View};

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
        super::tool_call_line(
            format!("write {path} — {}", status_label(input.status)),
            input,
        )
    }

    fn render_result(&self, input: ToolResultRenderInput<'_>) -> View {
        if input.is_error() {
            return View::spacer(0);
        }

        let mut children = vec![
            super::text(input.text, result_style(false))
                .fill_width()
                .into_view(),
        ];
        if let Some(diff) = super::structured_diff_view(input.details) {
            children.push(diff);
        }
        super::tool_result_block(column(children))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ToolOutcome, ToolResultRenderInput};
    use iyon_tui::{ColorSpec, OverflowIndicator, StyleSpec};

    #[test]
    fn successful_result_keeps_summary_before_structured_diff() {
        let details = serde_json::json!({
            "diff": "--- a/file\n+++ b/file\n@@ -0,0 +1,2 @@\n+hello\n+world\n"
        });
        let view = WriteRenderer.render_result(ToolResultRenderInput {
            tool_name: "write",
            text: "wrote file",
            details: &details,
            outcome: ToolOutcome::Success,
        });
        let lines = iyon_tui::testing::compile_view_lines(&view, 80);
        let summary = lines.iter().position(|line| line.contains("wrote file"));
        let header = lines
            .iter()
            .position(|line| line.contains("@@ -0,0 +1,2 @@"));
        let addition = lines.iter().position(|line| line.contains("+hello"));
        assert!(summary < header && header < addition);
        assert!(
            lines
                .iter()
                .filter(|line| !line.trim().is_empty())
                .all(|line| line.starts_with("  ")),
            "write result must share the bash hanging gutter, got {lines:?}"
        );
    }

    #[test]
    fn successful_addition_uses_the_product_diff_style() {
        let details = serde_json::json!({
            "diff": "@@ -0,0 +1 @@\n+new\n"
        });
        let view = WriteRenderer.render_result(ToolResultRenderInput {
            tool_name: "write",
            text: "wrote",
            details: &details,
            outcome: ToolOutcome::Success,
        });
        let style =
            iyon_tui::testing::style_at_text(&view, 80, &crate::tui::theme::iyon_theme(), "new");
        assert_eq!(style.fg_rgb, Some((104, 211, 145)));
    }

    #[test]
    fn structured_diff_is_not_clamped_by_the_write_renderer() {
        let body: String = (1..=20).map(|line| format!("+line {line}\n")).collect();
        let diff = format!("@@ -0,0 +1,20 @@\n{body}");
        let details = serde_json::json!({ "diff": diff });
        let full = WriteRenderer.render_result(ToolResultRenderInput {
            tool_name: "write",
            text: "wrote",
            details: &details,
            outcome: ToolOutcome::Success,
        });
        assert!(
            iyon_tui::testing::compile_view_lines(&full, 80).len()
                > usize::from(crate::tools::MAX_COLLAPSED_TOOL_ROWS)
        );

        let clamped = full.clamp_rows(
            crate::tools::MAX_COLLAPSED_TOOL_ROWS,
            OverflowIndicator::Footer {
                prefix: "… more lines (full result retained)".into(),
                style: StyleSpec::new()
                    .foreground(ColorSpec::theme("truncation_footer"))
                    .into(),
            },
        );
        assert_eq!(
            iyon_tui::testing::compile_view_lines(&clamped, 80).len(),
            usize::from(crate::tools::MAX_COLLAPSED_TOOL_ROWS)
        );
    }

    #[test]
    fn error_results_do_not_dump_the_written_content() {
        let details = serde_json::json!({
            "diff": "@@ -0,0 +1 @@\n+secret payload\n"
        });
        let view = WriteRenderer.render_result(ToolResultRenderInput {
            tool_name: "write",
            text: "failed to write file: secret payload",
            details: &details,
            outcome: ToolOutcome::Error,
        });
        let lines = iyon_tui::testing::compile_view_lines(&view, 80);
        assert!(!lines.iter().any(|line| line.contains("secret payload")));
        assert!(!lines.iter().any(|line| line.contains("write failed")));
    }
}
