use ratatui::{
    style::{Color, Style},
    text::Line,
};

use crate::transcript::wrap::{TranscriptCommitBoundary, wrap_transcript_rows};

#[derive(Debug, Clone)]
pub(crate) enum TimelineItem {
    UserMessage {
        text: String,
    },
    AssistantMessage {
        text: String,
    },
    ErrorMessage {
        text: String,
    },
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ToolTimelineStatus,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolTimelineStatus {
    PendingApproval,
    Running,
    Approved,
    Rejected,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TuiFormatter;

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> Vec<Line<'static>> {
        match item {
            TimelineItem::UserMessage { text } => {
                self.format_text_message(text, Style::default().bg(Color::Rgb(45, 55, 72)), true)
            }
            TimelineItem::AssistantMessage { text } => self.format_assistant_message(text, true),
            TimelineItem::ErrorMessage { text } => {
                self.format_text_message(text, Style::default().fg(Color::Red), true)
            }
            TimelineItem::ToolCall {
                tool_name,
                arguments,
                status,
                ..
            } => self.format_tool_call(tool_name, arguments, *status),
            TimelineItem::ToolResult {
                tool_name,
                text,
                details,
                is_error,
                ..
            } => self.format_tool_result(tool_name, text, details, *is_error),
        }
    }

    pub(crate) fn format_assistant_message(
        &self,
        text: &str,
        include_trailing_blank: bool,
    ) -> Vec<Line<'static>> {
        self.format_text_message(text, Style::default(), include_trailing_blank)
    }

    pub(crate) fn format_assistant_body(&self, text: &str) -> Vec<Line<'static>> {
        Self::format_text_body(text, Style::default())
    }

    fn format_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        status: ToolTimelineStatus,
    ) -> Vec<Line<'static>> {
        let style = tool_style(status);
        let status = tool_status_label(status);
        let mut rows = vec![
            Line::from(""),
            Line::styled(format_tool_call_title(tool_name, arguments, status), style),
        ];
        let preview = format_tool_call_preview(tool_name, arguments);
        if !preview.is_empty() {
            rows.extend(
                preview
                    .lines()
                    .map(|line| Line::styled(format!("  {line}"), style)),
            );
        }
        rows
    }

    fn format_tool_result(
        &self,
        tool_name: &str,
        text: &str,
        details: &serde_json::Value,
        is_error: bool,
    ) -> Vec<Line<'static>> {
        let style = if is_error {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Rgb(113, 128, 150))
        };
        let mut rows = format_tool_specific_result(tool_name, text, details, is_error, style);
        rows.push(Line::from(""));
        rows
    }

    fn format_text_message(
        &self,
        text: &str,
        style: Style,
        include_trailing_blank: bool,
    ) -> Vec<Line<'static>> {
        let mut rows = Vec::new();
        rows.push(Line::styled("", style));
        rows.extend(Self::format_text_body(text, style));
        if include_trailing_blank {
            rows.push(Line::styled("", style));
        }
        rows
    }

    fn format_text_body(text: &str, style: Style) -> Vec<Line<'static>> {
        styled_lines_preserving_newlines(text, style)
    }
}

fn tool_style(status: ToolTimelineStatus) -> Style {
    match status {
        ToolTimelineStatus::Failed | ToolTimelineStatus::Rejected => {
            Style::default().fg(Color::Red)
        }
        ToolTimelineStatus::Finished | ToolTimelineStatus::Approved => {
            Style::default().fg(Color::Rgb(104, 211, 145))
        }
        ToolTimelineStatus::PendingApproval => Style::default().fg(Color::Yellow),
        ToolTimelineStatus::Running => Style::default().fg(Color::Rgb(160, 174, 192)),
    }
}

fn tool_status_label(status: ToolTimelineStatus) -> &'static str {
    match status {
        ToolTimelineStatus::PendingApproval => "waiting for approval",
        ToolTimelineStatus::Running => "running",
        ToolTimelineStatus::Approved => "approved",
        ToolTimelineStatus::Rejected => "rejected",
        ToolTimelineStatus::Finished => "finished",
        ToolTimelineStatus::Failed => "failed",
    }
}

fn format_tool_call_title(tool_name: &str, args: &serde_json::Value, status: &str) -> String {
    match tool_name {
        "bash" => format!(
            "● $ {} — {status}",
            string_arg(args, "command").unwrap_or_default()
        ),
        "read" => format!(
            "● read {}{} — {status}",
            string_arg(args, "path").unwrap_or("..."),
            range_suffix(args)
        ),
        "write" => format!(
            "● write {} — {status}",
            string_arg(args, "path").unwrap_or("...")
        ),
        "edit" => format!(
            "● edit {} — {status}",
            string_arg(args, "path").unwrap_or("...")
        ),
        "grep" => format!(
            "● grep /{}/ in {} — {status}",
            string_arg(args, "pattern").unwrap_or(""),
            string_arg(args, "path").unwrap_or(".")
        ),
        "find" => format!(
            "● find {} in {} — {status}",
            string_arg(args, "pattern").unwrap_or(""),
            string_arg(args, "path").unwrap_or(".")
        ),
        "ls" => format!(
            "● ls {} — {status}",
            string_arg(args, "path").unwrap_or(".")
        ),
        _ => format!("● tool {tool_name} — {status}"),
    }
}

fn format_tool_call_preview(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "write" => string_arg(args, "content")
            .map(truncate_preview_text)
            .unwrap_or_default(),
        "edit" => args.get("edits").map(compact_json).unwrap_or_default(),
        "bash" | "read" | "grep" | "find" | "ls" => String::new(),
        _ => compact_json(args),
    }
}

fn format_tool_specific_result(
    tool_name: &str,
    text: &str,
    details: &serde_json::Value,
    is_error: bool,
    style: Style,
) -> Vec<Line<'static>> {
    match tool_name {
        "edit" if !is_error => format_edit_result(text, details, style),
        "bash" => format_bash_result(text, details, is_error, style),
        _ => format_generic_tool_result(tool_name, text, is_error, style),
    }
}

fn format_generic_tool_result(
    tool_name: &str,
    text: &str,
    is_error: bool,
    style: Style,
) -> Vec<Line<'static>> {
    let title = if is_error { "failed" } else { "result" };
    let mut rows = vec![Line::styled(format!("  {tool_name} {title}"), style)];
    rows.extend(indented_body(text, style));
    rows
}

fn format_bash_result(
    text: &str,
    details: &serde_json::Value,
    is_error: bool,
    style: Style,
) -> Vec<Line<'static>> {
    let title = if is_error {
        "bash failed"
    } else {
        "bash result"
    };
    let mut rows = vec![Line::styled(format!("  {title}"), style)];
    rows.extend(indented_body(text, style));
    if let Some(path) = details
        .get("fullOutputPath")
        .and_then(serde_json::Value::as_str)
    {
        rows.push(Line::styled(
            format!("  [Full output: {path}]"),
            Style::default().fg(Color::Yellow),
        ));
    }
    rows
}

fn format_edit_result(text: &str, details: &serde_json::Value, style: Style) -> Vec<Line<'static>> {
    let mut rows = vec![Line::styled(format!("  {text}"), style)];
    if let Some(diff) = details.get("diff").and_then(serde_json::Value::as_str) {
        rows.extend(diff.lines().map(format_diff_line));
    }
    rows
}

fn format_diff_line(line: &str) -> Line<'static> {
    let style = if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Rgb(113, 128, 150))
    };
    Line::styled(format!("  {line}"), style)
}

fn indented_body(text: &str, style: Style) -> Vec<Line<'static>> {
    styled_lines_preserving_newlines(text, style)
        .into_iter()
        .map(|line| {
            let text = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();
            Line::styled(format!("  {text}"), style)
        })
        .collect()
}

fn string_arg<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(serde_json::Value::as_str)
}

fn range_suffix(args: &serde_json::Value) -> String {
    let offset = args.get("offset").and_then(serde_json::Value::as_u64);
    let limit = args.get("limit").and_then(serde_json::Value::as_u64);
    match (offset, limit) {
        (Some(offset), Some(limit)) => format!(":{offset}-{}", offset + limit.saturating_sub(1)),
        (Some(offset), None) => format!(":{offset}"),
        _ => String::new(),
    }
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
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

#[derive(Debug, Clone)]
pub(crate) struct TranscriptState {
    canonical_items: Vec<TimelineItem>,
    logical_rows_cache: Vec<Line<'static>>,
    rendered_rows_cache: Option<TranscriptRenderCache>,
    commit_boundary: TranscriptCommitBoundary,
    formatter: TuiFormatter,
    assistant_stream_open: bool,
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            canonical_items: Vec::new(),
            logical_rows_cache: Vec::new(),
            rendered_rows_cache: None,
            commit_boundary: TranscriptCommitBoundary::default(),
            formatter: TuiFormatter,
            assistant_stream_open: false,
        }
    }
}

impl TranscriptState {
    pub(crate) fn push_item(&mut self, item: TimelineItem) {
        self.assistant_stream_open = false;
        self.canonical_items.push(item);
        self.rebuild_logical_rows_cache();
    }

    pub(crate) fn upsert_tool_call(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ToolTimelineStatus,
    ) {
        self.assistant_stream_open = false;
        if let Some(TimelineItem::ToolCall {
            tool_name: existing_name,
            arguments: existing_arguments,
            status: existing_status,
            ..
        }) = self.canonical_items.iter_mut().find(|item| {
            matches!(
                item,
                TimelineItem::ToolCall { tool_call_id: existing, .. } if existing == &tool_call_id
            )
        }) {
            if !tool_name.is_empty() {
                *existing_name = tool_name;
            }
            *existing_arguments = arguments;
            *existing_status = status;
        } else {
            self.canonical_items.push(TimelineItem::ToolCall {
                tool_call_id,
                tool_name,
                arguments,
                status,
            });
        }
        self.rebuild_logical_rows_cache();
    }

    pub(crate) fn update_tool_call_status(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        status: ToolTimelineStatus,
    ) {
        if let Some(TimelineItem::ToolCall {
            tool_name: existing_name,
            status: existing_status,
            ..
        }) = self.canonical_items.iter_mut().find(|item| {
            matches!(
                item,
                TimelineItem::ToolCall { tool_call_id: existing, .. } if existing == &tool_call_id
            )
        }) {
            if !tool_name.is_empty() {
                *existing_name = tool_name;
            }
            *existing_status = status;
            self.rebuild_logical_rows_cache();
        }
    }

    pub(crate) fn finish_tool_call(&mut self, tool_call_id: &str, is_error: bool) {
        let status = if is_error {
            ToolTimelineStatus::Failed
        } else {
            ToolTimelineStatus::Finished
        };
        if let Some(TimelineItem::ToolCall {
            status: existing_status,
            ..
        }) = self.canonical_items.iter_mut().find(|item| {
            matches!(
                item,
                TimelineItem::ToolCall { tool_call_id: existing, .. } if existing == tool_call_id
            )
        }) {
            *existing_status = status;
            self.rebuild_logical_rows_cache();
        }
    }

    pub(crate) fn push_tool_result(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
    ) {
        self.assistant_stream_open = false;
        self.canonical_items.push(TimelineItem::ToolResult {
            tool_call_id,
            tool_name,
            text,
            details,
            is_error,
        });
        self.rebuild_logical_rows_cache();
    }

    pub(crate) fn append_assistant_fragment(&mut self, text: String) {
        self.append_assistant_text(text, false);
    }

    pub(crate) fn append_assistant_stream_fragment(&mut self, text: String) {
        self.append_assistant_text(text, true);
    }

    pub(crate) fn finish_assistant_stream(&mut self) {
        if !self.assistant_stream_open {
            return;
        }
        self.assistant_stream_open = false;
        self.rebuild_logical_rows_cache();
    }

    fn append_assistant_text(&mut self, text: String, stream_open: bool) {
        if text.is_empty() {
            return;
        }

        match self.canonical_items.last_mut() {
            Some(TimelineItem::AssistantMessage { text: existing }) => existing.push_str(&text),
            _ => self
                .canonical_items
                .push(TimelineItem::AssistantMessage { text }),
        }
        self.assistant_stream_open = stream_open;
        self.rebuild_logical_rows_cache();
    }

    fn rebuild_logical_rows_cache(&mut self) {
        self.logical_rows_cache.clear();
        let open_assistant_idx = self.assistant_stream_open.then(|| {
            self.canonical_items
                .iter()
                .rposition(|item| matches!(item, TimelineItem::AssistantMessage { .. }))
        });
        for (idx, item) in self.canonical_items.iter().enumerate() {
            match item {
                TimelineItem::AssistantMessage { text }
                    if open_assistant_idx == Some(Some(idx)) =>
                {
                    // While an assistant stream is open, the transcript owns only the
                    // frozen/spilled portion of that assistant message. Keep the normal
                    // message top padding so the gap from the preceding user message is
                    // stable, but omit the official bottom padding: the live active pane
                    // owns the active/input margin until the stream is finalized.
                    let mut rows = self.formatter.format_assistant_message(text, false);

                    // The frozen transcript and live active pane touch at this boundary.
                    // If the spilled fragment currently ends in an empty rendered row,
                    // suppress exactly one such row so the seam does not show a moving
                    // blank gap while streaming. Finalized messages are rebuilt through
                    // the normal formatter path and regain their bottom padding.
                    drop_trailing_empty_row(&mut rows);
                    self.logical_rows_cache.extend(rows);
                }
                _ => self.logical_rows_cache.extend(self.formatter.format(item)),
            }
        }
        self.rendered_rows_cache = None;
    }

    pub(crate) fn ensure_render_cache(&mut self, width: u16) {
        let should_recompute = match self.rendered_rows_cache.as_ref() {
            Some(cache) => cache.width != width,
            None => true,
        };

        if !should_recompute {
            return;
        }

        let cache = TranscriptRenderCache::from_logical_rows(
            width.max(1),
            &self.logical_rows_cache,
            self.commit_boundary,
        );
        self.rendered_rows_cache = Some(cache);
    }

    pub(crate) fn uncommitted_rows(&self) -> &[Line<'static>] {
        let Some(cache) = self.rendered_rows_cache.as_ref() else {
            return &[];
        };

        &cache.rows
    }

    pub(crate) fn uncommitted_len(&self) -> usize {
        self.uncommitted_rows().len()
    }

    pub(crate) fn mark_rows_committed(&mut self, rows: usize) {
        let Some(cache) = self.rendered_rows_cache.as_mut() else {
            return;
        };

        let committed_rows = rows.min(cache.rows.len());
        if committed_rows == 0 {
            return;
        }

        self.commit_boundary = cache.row_end_boundaries[committed_rows - 1];
        cache.rows.drain(..committed_rows);
        cache.row_end_boundaries.drain(..committed_rows);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TranscriptRenderCache {
    width: u16,
    rows: Vec<Line<'static>>,
    row_end_boundaries: Vec<TranscriptCommitBoundary>,
}

impl TranscriptRenderCache {
    fn from_logical_rows(
        width: u16,
        logical_rows: &[Line<'static>],
        commit_boundary: TranscriptCommitBoundary,
    ) -> Self {
        let wrapped = wrap_transcript_rows(width, logical_rows, commit_boundary);

        Self {
            width,
            rows: wrapped.rows,
            row_end_boundaries: wrapped.row_end_boundaries,
        }
    }
}

fn styled_lines_preserving_newlines(text: &str, style: Style) -> Vec<Line<'static>> {
    text.split('\n')
        .map(|line| Line::styled(line.to_string(), style))
        .collect()
}

fn drop_trailing_empty_row(rows: &mut Vec<Line<'static>>) {
    let Some(last) = rows.last() else {
        return;
    };

    let is_empty = last
        .spans
        .iter()
        .all(|span| span.content.as_ref().is_empty());
    if is_empty {
        rows.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn formatter_preserves_multiline_message_shape() {
        let formatter = TuiFormatter;
        let item = TimelineItem::UserMessage {
            text: "line1\nline2\n".to_string(),
        };

        let lines = formatter.format(&item);
        let text_rows = lines.iter().map(line_text).collect::<Vec<_>>();

        assert_eq!(text_rows, vec!["", "line1", "line2", "", ""]);
    }

    #[test]
    fn wraps_transcript_rows_to_visual_width() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            text: "abcdef".to_string(),
        });

        transcript.ensure_render_cache(3);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["", "abc", "def", ""]);
    }

    #[test]
    fn commit_boundary_survives_resize_after_partial_line_commit() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            text: "abcdef".to_string(),
        });

        transcript.ensure_render_cache(3);
        transcript.mark_rows_committed(2);
        transcript.ensure_render_cache(2);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["de", "f", ""]);
    }
}
