use crate::tools::{
    registry::ToolRenderer,
    renderers::{column, result_lines, result_style},
    types::{ToolCallRenderInput, ToolResultRenderInput},
};
use iyon_tui::{IntoView, View};

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
        let mut children = vec![super::tool_call_line(
            format!("edit {path} — {}", status_label(input.status)),
            input,
        )];
        // Debug-only: surface the raw edits payload; the normal view keeps the
        // call compact and lets the result's diff carry the detail.
        if input.show_arg_preview
            && !matches!(
                input.status,
                crate::transcript::ToolTimelineStatus::Failed
                    | crate::transcript::ToolTimelineStatus::Rejected
                    | crate::transcript::ToolTimelineStatus::Cancelled
            )
            && let Some(edits) = input.arguments.get("edits")
        {
            let preview = serde_json::to_string_pretty(edits).unwrap_or_else(|_| edits.to_string());
            children.extend(result_lines(&preview, super::tool_style(input.status)));
        }
        column(children)
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
            "diff": "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"
        });
        let view = EditRenderer.render_result(ToolResultRenderInput {
            tool_name: "edit",
            text: "edited file",
            details: &details,
            outcome: ToolOutcome::Success,
        });
        let lines = iyon_tui::testing::compile_view_lines(&view, 80);
        let summary = lines.iter().position(|line| line.contains("edited file"));
        let header = lines.iter().position(|line| line.contains("@@ -1 +1 @@"));
        let addition = lines.iter().position(|line| line.contains("+new"));
        assert!(summary < header && header < addition);
        assert!(
            lines
                .iter()
                .filter(|line| !line.trim().is_empty())
                .all(|line| line.starts_with("  ")),
            "edit result must share the bash hanging gutter, got {lines:?}"
        );
    }

    #[test]
    fn successful_addition_uses_the_product_diff_style() {
        let details = serde_json::json!({
            "diff": "@@ -1 +1 @@\n-old\n+new\n"
        });
        let view = EditRenderer.render_result(ToolResultRenderInput {
            tool_name: "edit",
            text: "edited",
            details: &details,
            outcome: ToolOutcome::Success,
        });
        let style =
            iyon_tui::testing::style_at_text(&view, 80, &crate::tui::theme::iyon_theme(), "new");
        assert_eq!(style.fg_rgb, Some((104, 211, 145)));
    }

    #[test]
    fn malformed_successful_diff_remains_visible_as_raw_text() {
        let details = serde_json::json!({ "diff": "not a unified diff\n+raw" });
        let view = EditRenderer.render_result(ToolResultRenderInput {
            tool_name: "edit",
            text: "edited",
            details: &details,
            outcome: ToolOutcome::Success,
        });
        let lines = iyon_tui::testing::compile_view_lines(&view, 80);
        assert!(lines.iter().any(|line| line.contains("not a unified diff")));
        assert!(lines.iter().any(|line| line.contains("+raw")));
    }

    #[test]
    fn error_results_do_not_dump_the_call_or_diff() {
        let details = serde_json::json!({ "diff": "@@ -1 +1 @@\n-old\n+raw" });
        let view = EditRenderer.render_result(ToolResultRenderInput {
            tool_name: "edit",
            text: "oldText not found in weather.py: \"#!/usr/bin/env python3\\n...\"",
            details: &details,
            outcome: ToolOutcome::Error,
        });
        let lines = iyon_tui::testing::compile_view_lines(&view, 80);
        assert!(!lines.iter().any(|line| line.contains("python3")));
        assert!(!lines.iter().any(|line| line.contains("+raw")));
        assert!(!lines.iter().any(|line| line.contains("edit failed")));
    }

    #[test]
    fn structured_diff_is_not_clamped_by_the_edit_renderer() {
        let body: String = (1..=20).map(|line| format!("+line {line}\n")).collect();
        let diff = format!("@@ -0,0 +1,20 @@\n{body}");
        let details = serde_json::json!({ "diff": diff });
        let full = EditRenderer.render_result(ToolResultRenderInput {
            tool_name: "edit",
            text: "edited",
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
}
