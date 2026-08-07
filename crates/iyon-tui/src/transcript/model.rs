use std::borrow::Cow;

use ratatui::{
    style::{Color, Style},
    text::Line,
};

use crate::{
    theme,
    tools::{ToolCallRenderInput, ToolRendererRegistry, ToolResultRenderInput},
    transcript::{
        markdown,
        markdown::RenderedRow,
        row::TranscriptRow,
        wrap::{TranscriptCommitBoundary, wrap_transcript_rows},
    },
};

/// The kind of an assistant-message segment. `Thinking` is streamed reasoning,
/// kept logically distinct from answer text so it can be styled, hidden, or
/// truncated independently (the core holds the complete text regardless).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentKind {
    Text,
    Thinking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AssistantSegment {
    Text(String),
    Thinking(String),
}

impl AssistantSegment {
    pub(crate) fn text(&self) -> &str {
        match self {
            AssistantSegment::Text(text) | AssistantSegment::Thinking(text) => text,
        }
    }
}

/// Returns the sub-range of `segments` covering bytes `[from_byte, to_byte)` of
/// the concatenated text, splitting segments at the boundaries. Defensive
/// char-boundary clamping ensures no segment text is cut mid-codepoint.
pub(crate) fn slice_segments(
    segments: &[AssistantSegment],
    from_byte: usize,
    to_byte: usize,
) -> Vec<AssistantSegment> {
    let from = from_byte.min(to_byte);
    let to = to_byte.max(from_byte);

    let mut out = Vec::new();
    let mut cursor = 0usize;
    for segment in segments {
        let text = segment.text();
        let segment_end = cursor + text.len();

        if segment_end <= from {
            cursor = segment_end;
            continue;
        }
        if cursor >= to {
            break;
        }

        let start = if from > cursor { from - cursor } else { 0 };
        let end = if to < segment_end {
            to - cursor
        } else {
            text.len()
        };
        let start = clamp_to_char_boundary(text, start);
        let end = char_boundary_after(text, end).max(start);

        if end > start {
            let piece = &text[start..end];
            match segment {
                AssistantSegment::Text(_) => out.push(AssistantSegment::Text(piece.to_string())),
                AssistantSegment::Thinking(_) => {
                    out.push(AssistantSegment::Thinking(piece.to_string()))
                }
            }
        }

        if segment_end >= to {
            break;
        }
        cursor = segment_end;
    }
    out
}

fn clamp_to_char_boundary(text: &str, mut pos: usize) -> usize {
    pos = pos.min(text.len());
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

fn char_boundary_after(text: &str, pos: usize) -> usize {
    let mut pos = pos.min(text.len());
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

/// Muted + italic styling for thinking segments, matching the muted feel of a
/// background reasoning trace.
pub(crate) fn thinking_style() -> Style {
    theme::thinking()
}

#[derive(Debug, Clone)]
pub(crate) enum TimelineItem {
    UserMessage {
        text: String,
    },
    AssistantMessage {
        segments: Vec<AssistantSegment>,
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
        /// Display collapse state: when true, the result renders truncated to
        /// [`DISPLAY_MAX_LINES`] with a footer hint. The full `text` is retained
        /// regardless, so toggling is a pure re-render.
        collapsed: bool,
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

#[derive(Debug, Clone, Default)]
pub(crate) struct TuiFormatter {
    tool_renderers: ToolRendererRegistry,
    /// Debug aid: render tool-call argument previews under the call header.
    /// Defaults off via `Default`; the normal UI keeps calls compact.
    show_arg_preview: bool,
}

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> Vec<TranscriptRow> {
        match item {
            TimelineItem::UserMessage { text } => {
                self.format_text_message(text, Style::default().bg(theme::user_bubble_bg()), true)
            }
            TimelineItem::AssistantMessage { segments } => {
                self.format_assistant_message(segments, true)
            }
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
                collapsed,
                ..
            } => self.format_tool_result(tool_name, text, details, *is_error, *collapsed),
        }
    }

    pub(crate) fn format_assistant_message(
        &self,
        segments: &[AssistantSegment],
        include_trailing_blank: bool,
    ) -> Vec<TranscriptRow> {
        let mut rows = vec![TranscriptRow::blank()];
        rows.extend(Self::format_segments_body(segments));
        if include_trailing_blank {
            rows.push(TranscriptRow::blank());
        }
        rows
    }

    /// Formats `segments` into logical rows, **including** per-row freeze metadata
    /// for the active (streaming) pane. Rows that hide markdown markers report
    /// `restricted = true` so the active pane freezes them whole-line; otherwise
    /// a partial freeze would render differently live vs. committed.
    pub(crate) fn format_assistant_body_meta(
        &self,
        segments: &[AssistantSegment],
    ) -> Vec<RenderedRow> {
        markdown::render_assistant(segments).rows
    }

    /// Formats `segments` into styled logical rows (one per `'\n'` boundary). Text
    /// segments are rendered as markdown (headings, lists, bold/italic/code);
    /// thinking segments pass through as muted + italic and are never markdown.
    fn format_segments_body(segments: &[AssistantSegment]) -> Vec<TranscriptRow> {
        markdown::render_assistant(segments)
            .rows
            .into_iter()
            .map(|rr| rr.row)
            .collect()
    }

    fn format_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        status: ToolTimelineStatus,
    ) -> Vec<TranscriptRow> {
        let style = tool_style(status);
        let status_label = tool_status_label(status);
        let rows = self.tool_renderers.render_call(ToolCallRenderInput {
            tool_name,
            arguments,
            status: status_label,
            style,
            show_arg_preview: self.show_arg_preview,
        });
        apply_call_truncation(rows)
    }

    fn format_tool_result(
        &self,
        tool_name: &str,
        text: &str,
        details: &serde_json::Value,
        is_error: bool,
        collapsed: bool,
    ) -> Vec<TranscriptRow> {
        let style = if is_error {
            theme::tool_error()
        } else {
            theme::muted()
        };
        let mut rows = self.tool_renderers.render_result(ToolResultRenderInput {
            tool_name,
            text,
            details,
            is_error,
            style,
        });
        if collapsed {
            rows = apply_result_truncation(rows);
        }
        rows
    }

    fn format_text_message(
        &self,
        text: &str,
        style: Style,
        include_trailing_blank: bool,
    ) -> Vec<TranscriptRow> {
        let mut rows = Vec::new();
        rows.push(TranscriptRow::new(Line::styled("", style)));
        rows.extend(Self::format_text_body(text, style));
        if include_trailing_blank {
            rows.push(TranscriptRow::new(Line::styled("", style)));
        }
        rows
    }

    fn format_text_body(text: &str, style: Style) -> Vec<TranscriptRow> {
        styled_lines_preserving_newlines(text, style)
            .into_iter()
            .map(TranscriptRow::new)
            .collect()
    }
}

/// Maximum number of tool-result rows shown in the collapsed (default) display
/// state. Longer results are sliced at render time with a footer hint; the full
/// text stays on the timeline item for a future expand/collapse interaction.
const DISPLAY_MAX_LINES: usize = 15;

/// Cuts a rendered tool-result block down to [`DISPLAY_MAX_LINES`] logical rows,
/// appending a styled footer noting how many lines were hidden. This is a pure,
/// tool-agnostic choke point: every renderer maps result text 1:1 to rows, so the
/// slice applies uniformly regardless of tool. Applies `Style::reset()` margins
/// guard via the footer's own indented row.
fn apply_result_truncation(mut rows: Vec<TranscriptRow>) -> Vec<TranscriptRow> {
    if rows.len() <= DISPLAY_MAX_LINES {
        return rows;
    }
    let hidden = rows.len() - DISPLAY_MAX_LINES;
    rows.truncate(DISPLAY_MAX_LINES);
    let noun = if hidden == 1 { "line" } else { "lines" };
    rows.push(TranscriptRow::tool_result(
        format!("… {hidden} more {noun} (full result retained)"),
        truncation_footer_style(),
    ));
    rows
}

/// Maximum number of argument-preview lines shown for a tool call (always keeps
/// the leading spacer + bullet header). Argument dumps (edit specs, write content,
/// generic JSON) can be large; the full call is not retained post-hoc, so the
/// footer notes the hidden count without a retention claim.
const DISPLAY_CALL_MAX_LINES: usize = 15;

/// Caps a rendered tool-call block: keeps the bullet header and at most
/// [`DISPLAY_CALL_MAX_LINES`] argument-preview lines, appending a footer hint.
fn apply_call_truncation(rows: Vec<TranscriptRow>) -> Vec<TranscriptRow> {
    // rows[0] is the bullet header; the rest is preview.
    const HEADER_ROWS: usize = 1;
    let keep = HEADER_ROWS.min(rows.len());
    if rows.len() <= keep + DISPLAY_CALL_MAX_LINES {
        return rows;
    }
    let hidden = rows.len() - keep - DISPLAY_CALL_MAX_LINES;
    let mut out = Vec::with_capacity(keep + DISPLAY_CALL_MAX_LINES + 1);
    out.extend(rows.into_iter().take(keep + DISPLAY_CALL_MAX_LINES));
    out.push(TranscriptRow::tool_result(
        format!(
            "… {hidden} more argument line{}",
            if hidden == 1 { "" } else { "s" }
        ),
        truncation_footer_style(),
    ));
    out
}

fn truncation_footer_style() -> Style {
    theme::truncation_footer()
}

fn tool_style(status: ToolTimelineStatus) -> Style {
    match status {
        ToolTimelineStatus::Failed | ToolTimelineStatus::Rejected => theme::tool_error(),
        ToolTimelineStatus::Finished | ToolTimelineStatus::Approved => theme::tool_finished(),
        ToolTimelineStatus::PendingApproval => Style::default().fg(Color::Yellow),
        ToolTimelineStatus::Running => theme::tool_running(),
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

#[derive(Debug, Clone)]
pub(crate) struct TranscriptState {
    canonical_items: Vec<TimelineItem>,
    logical_rows_cache: Vec<TranscriptRow>,
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
            formatter: TuiFormatter::default(),
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
            collapsed: true,
        });
        self.rebuild_logical_rows_cache();
    }

    pub(crate) fn append_assistant_fragment(&mut self, segments: Vec<AssistantSegment>) {
        self.append_assistant_segments(segments, false);
    }

    pub(crate) fn append_assistant_stream_fragment(&mut self, segments: Vec<AssistantSegment>) {
        self.append_assistant_segments(segments, true);
    }

    pub(crate) fn finish_assistant_stream(&mut self) {
        if !self.assistant_stream_open {
            return;
        }
        self.assistant_stream_open = false;
        self.rebuild_logical_rows_cache();
    }

    #[cfg(test)]
    pub(crate) fn test_items(&self) -> &[TimelineItem] {
        &self.canonical_items
    }

    fn append_assistant_segments(&mut self, incoming: Vec<AssistantSegment>, stream_open: bool) {
        if incoming.is_empty() {
            return;
        }

        match self.canonical_items.last_mut() {
            Some(TimelineItem::AssistantMessage { segments }) => {
                for segment in incoming {
                    push_segment(segments, segment);
                }
            }
            _ => {
                // Newest assistant message. Route every segment through the central
                // `push_segment` too, so the thinking-to-text newline rule applies
                // uniformly whether this is a fresh message or a continuation.
                let mut segments = Vec::new();
                for segment in incoming {
                    push_segment(&mut segments, segment);
                }
                self.canonical_items
                    .push(TimelineItem::AssistantMessage { segments });
            }
        }
        self.assistant_stream_open = stream_open;
        self.rebuild_logical_rows_cache();
    }

    fn rebuild_logical_rows_cache(&mut self) {
        self.logical_rows_cache.clear();
        let last_assistant_idx = self
            .canonical_items
            .iter()
            .rposition(|item| matches!(item, TimelineItem::AssistantMessage { .. }));
        let open_assistant_idx = self
            .assistant_stream_open
            .then_some(last_assistant_idx)
            .flatten();
        debug_assert!(
            open_assistant_idx.is_none() || open_assistant_idx == last_assistant_idx,
            "open streaming assistant must be the last assistant message"
        );
        // Spacing between parts is owned centrally here. Adjacent items render with
        // exactly one blank separator row between them, EXCEPT when they form a single
        // logical unit: a `ToolResult` that immediately follows its matching `ToolCall`
        // (same `tool_call_id`) is a continuation of that call, so they touch with no
        // blank between. Consecutive user messages (a steered batch) fuse into one
        // bubble with no blank as well.
        let mut prev_is_user = false;
        for idx in 0..self.canonical_items.len() {
            let (rows, is_user, is_tool_result_of_prev_call) = {
                let item = &self.canonical_items[idx];
                let is_user = matches!(item, TimelineItem::UserMessage { .. });
                let is_open = matches!(
                    item,
                    TimelineItem::AssistantMessage { .. } if open_assistant_idx == Some(idx)
                );
                // A ToolResult is a continuation of the preceding item when that item
                // is a ToolCall with the same id — they are one logical tool unit.
                let is_tool_result_of_prev_call = matches!(
                    item,
                    TimelineItem::ToolResult { tool_call_id, .. }
                        if matches!(
                            idx.checked_sub(1).map(|i| &self.canonical_items[i]),
                            Some(TimelineItem::ToolCall { tool_call_id: prev, .. })
                                if prev == tool_call_id
                        )
                );
                let rows = match item {
                    TimelineItem::AssistantMessage { segments } if is_open => {
                        // While an assistant stream is open, the transcript owns only the
                        // frozen/spilled portion of that assistant message. Keep the normal
                        // message top padding so the gap from the preceding user message is
                        // stable, but omit the official bottom padding: the live active pane
                        // owns the active/input margin until the stream is finalized.
                        let mut rows = self.formatter.format_assistant_message(segments, false);

                        // The frozen transcript and live active pane touch at this boundary.
                        // If the spilled fragment currently ends in an empty rendered row,
                        // suppress exactly one such row so the seam does not show a moving
                        // blank gap while streaming. Finalized messages are rebuilt through
                        // the normal formatter path and regain their bottom padding.
                        drop_trailing_empty_row(&mut rows);
                        rows
                    }
                    _ => self.formatter.format(item),
                };
                (rows, is_user, is_tool_result_of_prev_call)
            };

            let is_second_user_in_run = is_user && prev_is_user;
            if is_second_user_in_run {
                // Fuse consecutive user bubbles into one: remove the inter-bubble
                // padding pair (prev trailing + next leading). Exactly one row
                // each — interior empty rows are content newlines and must stay.
                self.logical_rows_cache.pop();
                self.logical_rows_cache.extend(rows.into_iter().skip(1));
            } else {
                // Continuation (tool result under its call) gets no separator; every
                // other boundary gets exactly one blank.
                self.append_sequenced(rows, !is_tool_result_of_prev_call);
            }
            prev_is_user = is_user;
        }

        // The finalized transcript must always end above the input pane with a single
        // plain blank row (the gap to its top separator), regardless of which item is
        // last. A trailing blank that carries a background (the user bubble's colored
        // bottom padding) is part of its item and stays; plain trailing blanks are
        // collapsed into the one gap. While a stream is open the live active pane owns
        // that bottom margin instead, so it is not added then.
        if open_assistant_idx.is_none() {
            // Pop plain trailing separators, but keep any backgrounded (bubble) blank
            // — it is part of the item's rendered padding.
            while let Some(last) = self.logical_rows_cache.last()
                && is_blank_row(last)
                && !has_background(last)
            {
                self.logical_rows_cache.pop();
            }
            // Always finish with a plain gap above the input separator; a colored
            // bubble padding row, if present, stays as its own padding and this gap
            // is added after it.
            if !self.logical_rows_cache.is_empty() {
                self.logical_rows_cache.push(TranscriptRow::blank());
            }
        }

        self.rendered_rows_cache = None;
    }

    /// Appends the rows of one timeline item to `logical_rows_cache`. When
    /// `separated` is true the junction is clamped so adjacent items are separated
    /// by exactly one blank row; when false the item is a continuation (e.g. a tool
    /// result directly under its call) and touches the previous content with no
    /// blank. A blank row that carries a background (the user bubble's colored
    /// padding) is treated as *part of its item*, not a separator, so an item's
    /// rendered padding stays with it and a fresh plain blank separates items. The
    /// first item keeps its natural leading/trailing padding.
    fn append_sequenced(&mut self, rows: Vec<TranscriptRow>, separated: bool) {
        if rows.is_empty() {
            return;
        }
        if self.logical_rows_cache.is_empty() {
            // First item: keep its own leading/trailing padding unchanged.
            self.logical_rows_cache.extend(rows);
            return;
        }

        if separated {
            // Drop the previous item's trailing blanks that are plain separators.
            // Blanks that carry a background (e.g. the user bubble's colored padding)
            // are *part of that item*, so they stay — the real inter-item gap is
            // added below, never touching the bubble.
            while let Some(last) = self.logical_rows_cache.last()
                && is_blank_row(last)
                && !has_background(last)
            {
                self.logical_rows_cache.pop();
            }

            // Drop the new item's leading blanks that are plain separators; any
            // backgrounded leading blank is that item's own bubble padding and stays.
            let mut rest = rows.as_slice();
            while let Some(first) = rest.first()
                && is_blank_row(first)
                && !has_background(first)
            {
                rest = &rest[1..];
            }
            if rest.is_empty() {
                // The new item contributed only separators; keep a single gap.
                if self.logical_rows_cache.is_empty()
                    || !is_blank_row(self.logical_rows_cache.last().unwrap())
                {
                    self.logical_rows_cache.push(TranscriptRow::blank());
                }
                return;
            }

            // Exactly one plain separator between the previous content (after its
            // own padding) and this item.
            self.logical_rows_cache.push(TranscriptRow::blank());
            self.logical_rows_cache.extend_from_slice(rest);
        } else {
            // Continuation: no separator; drop plain leading separators but keep any
            // backgrounded (bubble) padding of the continuing item.
            self.logical_rows_cache.extend(
                rows.into_iter()
                    .skip_while(|r| is_blank_row(r) && !has_background(r)),
            );
        }
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
        logical_rows: &[TranscriptRow],
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

fn is_blank_row(row: &TranscriptRow) -> bool {
    row.line
        .spans
        .iter()
        .all(|span| span.content.as_ref().is_empty())
}

/// True when a row carries a background style (e.g. the user bubble's colored
/// padding rows). Such a blank belongs to its item as rendered padding, never a
/// separator, so it is preserved by the assembly pass.
fn has_background(row: &TranscriptRow) -> bool {
    row.line.style.bg.is_some() || row.line.spans.iter().any(|span| span.style.bg.is_some())
}

fn drop_trailing_empty_row(rows: &mut Vec<TranscriptRow>) {
    let Some(last) = rows.last() else {
        return;
    };

    if is_blank_row(last) {
        rows.pop();
    }
}

/// The single, central spacing rule that an assistant's answer text starts on
/// its own blank line after a reasoning segment: returns `chunk` with two
/// leading `\n`s prepended when it is the first `Text` arriving after a
/// `Thinking` segment that does not already end with a blank line. Idempotent
/// (it skips chunks that already lead with a newline), so it is safe to apply at
/// both the live stream (`push_delta`) and the transcript assembly
/// (`push_segment`) without ever double-inserting.
///
/// Two `\n`s (an empty line) are needed so the rendered transcript gets a real
/// blank row between thinking and answer — a single `\n` is just a line break
/// with no visible gap, and a blank row must be backed by actual source bytes
/// to keep the freeze/spill byte math exact.
pub(crate) fn think_to_text_newline<'a>(
    segments: &[AssistantSegment],
    kind: SegmentKind,
    chunk: &'a str,
) -> Cow<'a, str> {
    if kind == SegmentKind::Text
        && !chunk.starts_with('\n')
        && matches!(
            segments.last(),
            Some(AssistantSegment::Thinking(text)) if !text.ends_with('\n')
        )
    {
        let mut s = String::with_capacity(chunk.len() + 2);
        s.push('\n');
        s.push('\n');
        s.push_str(chunk);
        Cow::Owned(s)
    } else {
        Cow::Borrowed(chunk)
    }
}

/// Appends `segment` to `target`, merging into the trailing segment when it is
/// the same kind so contiguous runs stay a single logical segment. The
/// thinking-to-text newline rule is applied centrally here so every assembly
/// path (streaming spills, finalize, historical batches) gets identical spacing.
fn push_segment(target: &mut Vec<AssistantSegment>, segment: AssistantSegment) {
    let kind = match segment {
        AssistantSegment::Text(_) => SegmentKind::Text,
        AssistantSegment::Thinking(_) => SegmentKind::Thinking,
    };
    let chunk = think_to_text_newline(target, kind, segment.text());

    match kind {
        SegmentKind::Text => {
            if let Some(AssistantSegment::Text(existing)) = target.last_mut() {
                existing.push_str(&chunk);
            } else {
                target.push(AssistantSegment::Text(chunk.into_owned()));
            }
        }
        SegmentKind::Thinking => {
            if let Some(AssistantSegment::Thinking(existing)) = target.last_mut() {
                existing.push_str(&chunk);
            } else {
                target.push(AssistantSegment::Thinking(chunk.into_owned()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    fn text_segments(text: &str) -> Vec<AssistantSegment> {
        vec![AssistantSegment::Text(text.to_string())]
    }

    #[test]
    fn formatter_preserves_multiline_message_shape() {
        let formatter = TuiFormatter::default();
        let item = TimelineItem::UserMessage {
            text: "line1\nline2\n".to_string(),
        };

        let lines = formatter.format(&item);
        let text_rows = lines
            .iter()
            .map(|row| line_text(&row.line))
            .collect::<Vec<_>>();

        assert_eq!(text_rows, vec!["", "line1", "line2", "", ""]);
    }

    #[test]
    fn wraps_transcript_rows_to_visual_width() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("abcdef"),
        });

        // width 6 minus outer margins (2+2) = content width 2.
        transcript.ensure_render_cache(6);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["", "  ab", "  cd", "  ef", ""]);
    }

    #[test]
    fn commit_boundary_survives_resize_after_partial_line_commit() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("abcdef"),
        });

        transcript.ensure_render_cache(6);
        transcript.mark_rows_committed(2);
        transcript.ensure_render_cache(6);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        // After committing the first two physical rows (blank + "  ab"), the
        // boundary lands after "ab"; the remaining content re-wraps at width 6
        // (content width 2) as "cd" / "ef" plus the trailing blank.
        assert_eq!(rows, ["  cd", "  ef", ""]);
    }

    #[test]
    fn thinking_segment_is_muted_and_italic() {
        let formatter = TuiFormatter::default();
        let rows = formatter
            .format_assistant_message(&[AssistantSegment::Thinking("rethink".to_string())], false);
        // rows[0] is the leading blank line; rows[1] is the thinking body line.
        let body = &rows[1].line;
        assert_eq!(body.spans.len(), 1);
        assert!(body.spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert_ne!(body.spans[0].style.fg, Some(ratatui::style::Color::Reset));
    }

    #[test]
    fn plain_text_segment_is_not_italic() {
        let formatter = TuiFormatter::default();
        let rows = formatter
            .format_assistant_body_meta(&text_segments("answer"))
            .into_iter()
            .map(|rr| rr.row)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert!(
            !rows[0].line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    #[test]
    fn mixed_segments_produce_one_line_with_distinct_span_styles() {
        let formatter = TuiFormatter::default();
        let rows = formatter
            .format_assistant_body_meta(&[
                AssistantSegment::Thinking("hmm".to_string()),
                AssistantSegment::Text("answer".to_string()),
            ])
            .into_iter()
            .map(|rr| rr.row)
            .collect::<Vec<_>>();

        assert_eq!(rows.len(), 1);
        assert_eq!(line_text(&rows[0].line), "hmmanswer");
        assert_eq!(rows[0].line.spans.len(), 2);
        assert!(
            rows[0].line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert!(
            !rows[0].line.spans[1]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    #[test]
    fn newline_splits_rows_but_keeps_segment_styles() {
        let formatter = TuiFormatter::default();
        // The text segment carries a leading blank line (two \n) as the stream
        // inserts between reasoning and the answer (see think_to_text_newline).
        let rows = formatter
            .format_assistant_body_meta(&[
                AssistantSegment::Thinking("a\nb".to_string()),
                AssistantSegment::Text("\n\nc".to_string()),
            ])
            .into_iter()
            .map(|rr| rr.row)
            .collect::<Vec<_>>();

        assert_eq!(rows.len(), 4);
        assert_eq!(line_text(&rows[0].line), "a");
        assert_eq!(line_text(&rows[1].line), "b");
        assert_eq!(line_text(&rows[2].line), "");
        assert_eq!(line_text(&rows[3].line), "c");
        assert!(
            rows[0].line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert!(
            rows[1].line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert!(
            !rows[3].line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    #[test]
    fn slice_whole_segment() {
        let segs = vec![AssistantSegment::Text("hello".to_string())];
        assert_eq!(
            slice_segments(&segs, 0, 5),
            vec![AssistantSegment::Text("hello".to_string())]
        );
    }

    #[test]
    fn slice_full_range_returns_all_segments() {
        let segs = vec![
            AssistantSegment::Thinking("xy".to_string()),
            AssistantSegment::Text("z".to_string()),
        ];
        assert_eq!(slice_segments(&segs, 0, usize::MAX), segs);
    }

    #[test]
    fn slice_empty_range_is_empty() {
        let segs = vec![AssistantSegment::Text("abc".to_string())];
        assert_eq!(slice_segments(&segs, 2, 2), Vec::<AssistantSegment>::new());
    }

    #[test]
    fn slice_lands_mid_segment() {
        let segs = vec![AssistantSegment::Text("abcde".to_string())];
        assert_eq!(
            slice_segments(&segs, 1, 4),
            vec![AssistantSegment::Text("bcd".to_string())]
        );
    }

    #[test]
    fn slice_across_segment_boundary_splits_both() {
        let segs = vec![
            AssistantSegment::Thinking("ab".to_string()),
            AssistantSegment::Text("cd".to_string()),
        ];
        // bytes: a=0 b=1 c=2 d=3 -> [1,3) = "b" (thinking) + "c" (text)
        assert_eq!(
            slice_segments(&segs, 1, 3),
            vec![
                AssistantSegment::Thinking("b".to_string()),
                AssistantSegment::Text("c".to_string())
            ]
        );
    }

    #[test]
    fn slice_skips_leading_whole_segments() {
        let segs = vec![
            AssistantSegment::Text("ab".to_string()),
            AssistantSegment::Thinking("cd".to_string()),
        ];
        assert_eq!(
            slice_segments(&segs, 2, 4),
            vec![AssistantSegment::Thinking("cd".to_string())]
        );
    }

    #[test]
    fn slice_clamps_to_unicode_char_boundaries() {
        // 'é' is two bytes: bytes 1..=2.
        let segs = vec![AssistantSegment::Text("héllo".to_string())];
        let got = slice_segments(&segs, 1, 6);
        assert_eq!(got, vec![AssistantSegment::Text("éllo".to_string())]);
    }

    #[test]
    fn slices_never_produce_an_empty_inner_segment() {
        let segs = vec![
            AssistantSegment::Thinking("abc".to_string()),
            AssistantSegment::Text("def".to_string()),
        ];
        let got = slice_segments(&segs, 0, 3);
        // Only the thinking segment is include (exact boundary, no empty text).
        assert_eq!(got, vec![AssistantSegment::Thinking("abc".to_string())]);
    }

    #[test]
    fn transcript_merges_adjacent_same_kind_segments() {
        let mut transcript = TranscriptState::default();
        transcript.append_assistant_fragment(vec![AssistantSegment::Text("a".to_string())]);
        transcript.append_assistant_stream_fragment(vec![AssistantSegment::Text("b".to_string())]);
        transcript.finish_assistant_stream();

        let Some(TimelineItem::AssistantMessage { segments }) = transcript.canonical_items.first()
        else {
            panic!("expected an assistant message item");
        };
        assert_eq!(segments, &vec![AssistantSegment::Text("ab".to_string())]);
    }

    #[test]
    fn transcript_keeps_thinking_and_text_separate() {
        let mut transcript = TranscriptState::default();
        transcript.append_assistant_fragment(vec![
            AssistantSegment::Thinking("t".to_string()),
            AssistantSegment::Text("a".to_string()),
        ]);
        transcript.append_assistant_stream_fragment(vec![AssistantSegment::Text("b".to_string())]);

        let Some(TimelineItem::AssistantMessage { segments }) = transcript.canonical_items.first()
        else {
            panic!("expected an assistant message item");
        };
        // Thinking and text stay separate, and the central think-to-text rule inserts
        // a blank line (two \n) when the answer begins after reasoning (matching the
        // live-stream path).
        assert_eq!(
            segments,
            &vec![
                AssistantSegment::Thinking("t".to_string()),
                AssistantSegment::Text("\n\nab".to_string())
            ]
        );
    }

    #[test]
    fn consecutive_user_messages_merge_into_one_bubble() {
        let mut transcript = TranscriptState::default();
        // A steered batch delivered together arrives as consecutive user messages.
        transcript.push_item(TimelineItem::UserMessage {
            text: "a".to_string(),
        });
        transcript.push_item(TimelineItem::UserMessage {
            text: "b".to_string(),
        });
        transcript.push_item(TimelineItem::UserMessage {
            text: "c".to_string(),
        });

        // Rendered as one contiguous block with the shared bubble's bottom padding
        // kept as its own colored row, followed by a plain gap above the input:
        // [leading blank, a, b, c, bubble-bottom-blank, plain-gap].
        let texts: Vec<String> = transcript
            .logical_rows_cache
            .iter()
            .map(|row| row.line.to_string())
            .collect();
        assert_eq!(texts, vec!["", "a", "b", "c", "", ""]);
    }

    #[test]
    fn user_message_after_tool_keeps_padding() {
        // A non-consecutive user message (here after an assistant) keeps its normal
        // bubble padding; the merge only applies to consecutive user messages.
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "a".to_string(),
        });
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: vec![AssistantSegment::Text("hi".to_string())],
        });
        transcript.push_item(TimelineItem::UserMessage {
            text: "b".to_string(),
        });

        let texts: Vec<String> = transcript
            .logical_rows_cache
            .iter()
            .map(|row| row.line.to_string())
            .collect();
        // [\n, a, \n, \n, hi, \n, \n, b, \n, \n] — a and b are separated by the
        // assistant message. Each item keeps its own bubble padding (backgrounded
        // blank) and a plain separator is added between items; the trailing user
        // bubble adds one more plain gap above the input separator.
        assert_eq!(texts, vec!["", "a", "", "", "hi", "", "", "b", "", ""]);
    }

    #[test]
    fn tool_result_collapses_to_display_max_lines_with_footer() {
        let mut transcript = TranscriptState::default();
        let text = (0..50)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        transcript.push_item(TimelineItem::ToolResult {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            text,
            details: serde_json::Value::Null,
            is_error: false,
            collapsed: true,
        });

        // Renderer emits 1 title row + 50 body rows = 51; collapsed keeps
        // DISPLAY_MAX_LINES + a footer hint, and the assembler adds a trailing
        // blank so the result clears the input separator.
        let rows = &transcript.logical_rows_cache;
        assert_eq!(rows.len(), 15 + 1 + 1, "15 kept + footer + trailing blank");

        let footer_text: String = rows[15]
            .line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(footer_text.contains("36 more lines"), "got {footer_text:?}");
    }

    #[test]
    fn short_tool_result_is_not_truncated() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::ToolResult {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            text: "one\ntwo".to_string(),
            details: serde_json::Value::Null,
            is_error: false,
            collapsed: true,
        });

        // 1 title + 2 body = 3 rows (under the cap), plus the assembler's trailing
        // blank so the result clears the input separator.
        let rows = &transcript.logical_rows_cache;
        assert_eq!(rows.len(), 3 + 1);
        let texts: Vec<String> = rows.iter().map(|r| r.line.to_string()).collect();
        assert_eq!(texts, vec!["read result", "one", "two", ""]);
    }

    #[test]
    fn tool_call_renders_compact_without_arg_payload() {
        let mut transcript = TranscriptState::default();
        // A JSON object with many keys: even so, the call renders as just the
        // header (blank spacer + bullet) — the argument payload is not shown in
        // the default view.
        let mut obj = serde_json::Map::new();
        for i in 0..30 {
            obj.insert(format!("key{i}"), serde_json::Value::String("v".into()));
        }
        transcript.push_item(TimelineItem::ToolCall {
            tool_call_id: "1".to_string(),
            tool_name: "*".to_string(), // generic renderer
            arguments: serde_json::Value::Object(obj),
            status: ToolTimelineStatus::Finished,
        });

        // Bullet header only — no argument dump, and no self-injected leading blank
        // (the assembler owns separators). A trailing blank clears the input separator.
        let rows = &transcript.logical_rows_cache;
        assert_eq!(rows.len(), 2);
        let texts: Vec<String> = rows.iter().map(|r| r.line.to_string()).collect();
        assert_eq!(texts, vec!["tool * — finished", ""]);
    }

    #[test]
    fn user_bubble_keeps_bottom_background_when_assistant_follows() {
        // The user bubble's bottom padding is a colored blank row; when the assistant
        // arrives it must persist (as the separator) rather than being swapped for a
        // plain blank — otherwise the bubble loses its lower background extension.
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "hello".to_string(),
        });
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: vec![AssistantSegment::Thinking("t".to_string())],
        });

        let rows = &transcript.logical_rows_cache;
        // structure: [colored-blank, hello, colored-blank(bubble bottom), plain
        // separator, t, trailing-blank] — the bubble's padded bottom row stays part of
        // the bubble, and a fresh plain gap separates it from the agent message.
        let texts: Vec<String> = rows.iter().map(|r| r.line.to_string()).collect();
        assert_eq!(texts, vec!["", "hello", "", "", "t", ""]);

        // The bubble's bottom padding (rows[2]) must keep the background; the gap
        // (rows[3]) is a plain separator.
        assert!(
            has_background(&rows[2]),
            "bubble bottom must keep its background"
        );
        assert!(
            !has_background(&rows[3]),
            "inter-item separator must be plain"
        );
    }

    #[test]
    fn user_bubble_keeps_bottom_background_as_last_item() {
        // A just-sent user message is the last item. Its colored bottom padding must
        // stay part of the bubble (not stripped), and a plain gap clears the input
        // separator above it.
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "hello".to_string(),
        });

        let rows = &transcript.logical_rows_cache;
        let texts: Vec<String> = rows.iter().map(|r| r.line.to_string()).collect();
        // [colored-blank, hello, colored-blank(bubble bottom), plain gap]
        assert_eq!(texts, vec!["", "hello", "", ""]);
        assert!(
            has_background(&rows[2]),
            "bubble bottom must keep its background"
        );
        assert!(!has_background(&rows[3]), "gap above input must be plain");
    }

    #[test]
    fn thinking_to_text_produces_a_real_blank_row() {
        // The primary bug: thinking and answer text must be separated by a visible
        // blank row, not just a line break. The central rule emits an empty line
        // (two \n) so the rendered assistant body has a blank row in between.
        let formatter = TuiFormatter::default();
        let rows = formatter
            .format_assistant_body_meta(&[
                AssistantSegment::Thinking("t".to_string()),
                AssistantSegment::Text("\n\nabc".to_string()),
            ])
            .into_iter()
            .collect::<Vec<_>>();

        let texts: Vec<String> = rows
            .iter()
            .map(|r| {
                r.row
                    .line
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect()
            })
            .collect();
        assert_eq!(texts, vec!["t", "", "abc"]);
    }

    #[test]
    fn tool_call_and_result_render_as_one_unit_no_blank_between() {
        // A ToolResult that immediately follows its matching ToolCall (same id) is a
        // logical unit: no blank row between the call bullet and the result. But a
        // separator appears before the *next* tool call.
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::ToolCall {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::Value::Null,
            status: ToolTimelineStatus::Finished,
        });
        transcript.push_item(TimelineItem::ToolResult {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            text: "one\ntwo".to_string(),
            details: serde_json::Value::Null,
            is_error: false,
            collapsed: true,
        });
        transcript.push_item(TimelineItem::ToolCall {
            tool_call_id: "2".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::Value::Null,
            status: ToolTimelineStatus::Finished,
        });

        let texts: Vec<String> = transcript
            .logical_rows_cache
            .iter()
            .map(|r| r.line.to_string())
            .collect();
        // [call1, result-title, one, two, SEPARATOR, call2, trailing-blank] — the call
        // and its own result touch with no blank, a single blank precedes the next call,
        // and the assembler's trailing blank clears the input separator.
        assert_eq!(
            texts,
            vec![
                "read ... — finished", // call1
                "read result",
                "one",
                "two",
                "",                    // separator
                "read ... — finished", // call2
                "",                    // trailing blank above input
            ]
        );
    }

    #[test]
    fn tool_result_wrapped_continuations_keep_indent() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::ToolResult {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            text: "short line here\naxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string(),
            details: serde_json::Value::Null,
            is_error: false,
            collapsed: true,
        });
        // Narrow terminal forces wrapping on the assistant/body lines.
        transcript.ensure_render_cache(8);

        let rows = transcript.uncommitted_rows();
        // Every rendered body line (and the title) keeps the 2-column hanging indent,
        // including wrapped continuation lines.
        for row in rows
            .iter()
            .filter(|l| l.spans.iter().any(|s| !s.content.is_empty()))
        {
            let text: String = row.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.starts_with("  "), "row lost indent: {text:?}");
        }
    }
}
