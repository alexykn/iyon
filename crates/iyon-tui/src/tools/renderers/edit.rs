use ratatui::style::{Color, Style};

use crate::{
    theme,
    tools::{
        registry::ToolRenderer,
        types::{ToolCallRenderInput, ToolResultRenderInput},
    },
    transcript::row::TranscriptRow,
};

#[derive(Debug)]
pub(crate) struct EditRenderer;

impl ToolRenderer for EditRenderer {
    fn tool_name(&self) -> &'static str {
        "edit"
    }

    fn render_call(&self, input: ToolCallRenderInput<'_>) -> Vec<TranscriptRow> {
        let path = input
            .arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("...");
        let mut rows = vec![
            TranscriptRow::blank(),
            TranscriptRow::bullet(format!("edit {path} — {}", input.status), input.style),
        ];
        // Debug-only: surface the raw edits payload; the normal view keeps the
        // call compact and lets the result's diff carry the detail.
        if input.show_arg_preview {
            if let Some(edits) = input.arguments.get("edits") {
                let preview =
                    serde_json::to_string_pretty(edits).unwrap_or_else(|_| edits.to_string());
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
        if input.is_error {
            let mut rows = vec![TranscriptRow::indented("edit failed", input.style)];
            rows.extend(
                input
                    .text
                    .split('\n')
                    .map(|line| TranscriptRow::indented(line.to_string(), input.style)),
            );
            return rows;
        }

        let mut rows = vec![TranscriptRow::indented(input.text, input.style)];
        if let Some(diff) = input
            .details
            .get("diff")
            .and_then(serde_json::Value::as_str)
        {
            rows.extend(diff.lines().map(format_diff_line));
        }
        rows
    }
}

/// Formats a single diff line with +/-/hunk coloring. The line carries its own
/// style both for its content and its indent, matching the previous behavior
/// where the leading spaces were part of the same styled line.
fn format_diff_line(line: &str) -> TranscriptRow {
    let style = if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Yellow)
    } else {
        theme::muted()
    };
    TranscriptRow::indented(line.to_string(), style)
}
