use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::{
    tools::{ToolCallRenderInput, ToolRendererRegistry, ToolResultRenderInput},
    transcript::wrap::{TranscriptCommitBoundary, wrap_transcript_rows},
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
        let end = if to < segment_end { to - cursor } else { text.len() };
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
    Style::default()
        .fg(Color::Rgb(113, 128, 150))
        .add_modifier(Modifier::ITALIC)
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
}

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> Vec<Line<'static>> {
        match item {
            TimelineItem::UserMessage { text } => {
                self.format_text_message(text, Style::default().bg(Color::Rgb(45, 55, 72)), true)
            }
            TimelineItem::AssistantMessage { segments } => self.format_assistant_message(segments, true),
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
        segments: &[AssistantSegment],
        include_trailing_blank: bool,
    ) -> Vec<Line<'static>> {
        let mut rows = vec![Line::from("")];
        rows.extend(Self::format_segments_body(segments));
        if include_trailing_blank {
            rows.push(Line::from(""));
        }
        rows
    }

    pub(crate) fn format_assistant_body(&self, segments: &[AssistantSegment]) -> Vec<Line<'static>> {
        Self::format_segments_body(segments)
    }

    /// Formats `segments` into logical rows, one per `'\n'` boundary. A single row
    /// may contain multiple spans when a line mixes thinking and answer text; each
    /// run is styled by its segment kind so thinking renders muted + italic.
    fn format_segments_body(segments: &[AssistantSegment]) -> Vec<Line<'static>> {
        let mut rows = Vec::new();
        let mut current: Vec<Span<'static>> = Vec::new();

        for segment in segments {
            let style = match segment {
                AssistantSegment::Text(_) => Style::default(),
                AssistantSegment::Thinking(_) => thinking_style(),
            };
            for (index, piece) in segment.text().split('\n').enumerate() {
                if index > 0 {
                    rows.push(Line::from(std::mem::take(&mut current)));
                }
                if !piece.is_empty() {
                    current.push(Span::styled(piece.to_string(), style));
                }
            }
        }

        if !current.is_empty() {
            rows.push(Line::from(current));
        }
        rows
    }

    fn format_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        status: ToolTimelineStatus,
    ) -> Vec<Line<'static>> {
        let style = tool_style(status);
        let status_label = tool_status_label(status);
        self.tool_renderers.render_call(ToolCallRenderInput {
            tool_name,
            arguments,
            status: status_label,
            style,
        })
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
        let mut rows = self.tool_renderers.render_result(ToolResultRenderInput {
            tool_name,
            text,
            details,
            is_error,
            style,
        });
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
            _ => self
                .canonical_items
                .push(TimelineItem::AssistantMessage { segments: incoming }),
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
                TimelineItem::AssistantMessage { segments }
                    if open_assistant_idx == Some(Some(idx)) =>
                {
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

/// Appends `segment` to `target`, merging into the trailing segment when it is
/// the same kind so contiguous runs stay a single logical segment.
fn push_segment(target: &mut Vec<AssistantSegment>, segment: AssistantSegment) {
    match segment {
        AssistantSegment::Text(text) => {
            if let Some(AssistantSegment::Text(existing)) = target.last_mut() {
                existing.push_str(&text);
            } else {
                target.push(AssistantSegment::Text(text));
            }
        }
        AssistantSegment::Thinking(text) => {
            if let Some(AssistantSegment::Thinking(existing)) = target.last_mut() {
                existing.push_str(&text);
            } else {
                target.push(AssistantSegment::Thinking(text));
            }
        }
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
        let text_rows = lines.iter().map(line_text).collect::<Vec<_>>();

        assert_eq!(text_rows, vec!["", "line1", "line2", "", ""]);
    }

    #[test]
    fn wraps_transcript_rows_to_visual_width() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("abcdef"),
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
            segments: text_segments("abcdef"),
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

    #[test]
    fn thinking_segment_is_muted_and_italic() {
        let formatter = TuiFormatter::default();
        let rows = formatter.format_assistant_message(
            &[AssistantSegment::Thinking("rethink".to_string())],
            false,
        );
        // rows[0] is the leading blank line; rows[1] is the thinking body line.
        let body = &rows[1];
        assert_eq!(body.spans.len(), 1);
        assert!(body.spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert_ne!(
            body.spans[0].style.fg,
            Some(ratatui::style::Color::Reset)
        );
    }

    #[test]
    fn plain_text_segment_is_not_italic() {
        let formatter = TuiFormatter::default();
        let rows = formatter.format_assistant_body(&text_segments("answer"));
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].spans[0].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn mixed_segments_produce_one_line_with_distinct_span_styles() {
        let formatter = TuiFormatter::default();
        let rows = formatter.format_assistant_body(&[
            AssistantSegment::Thinking("hmm".to_string()),
            AssistantSegment::Text("answer".to_string()),
        ]);

        assert_eq!(rows.len(), 1);
        assert_eq!(line_text(&rows[0]), "hmmanswer");
        assert_eq!(rows[0].spans.len(), 2);
        assert!(rows[0].spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert!(!rows[0].spans[1].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn newline_splits_rows_but_keeps_segment_styles() {
        let formatter = TuiFormatter::default();
        // The text segment carries a leading newline, as the stream inserts between
        // reasoning and the answer (see ActiveStreamState::push_delta).
        let rows = formatter.format_assistant_body(&[
            AssistantSegment::Thinking("a\nb".to_string()),
            AssistantSegment::Text("\nc".to_string()),
        ]);

        assert_eq!(rows.len(), 3);
        assert_eq!(line_text(&rows[0]), "a");
        assert_eq!(line_text(&rows[1]), "b");
        assert_eq!(line_text(&rows[2]), "c");
        assert!(rows[0].spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert!(rows[1].spans[0].style.add_modifier.contains(Modifier::ITALIC));
        assert!(!rows[2].spans[0].style.add_modifier.contains(Modifier::ITALIC));
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
        assert_eq!(
            got,
            vec![AssistantSegment::Text("éllo".to_string())]
        );
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
        assert_eq!(
            segments,
            &vec![
                AssistantSegment::Thinking("t".to_string()),
                AssistantSegment::Text("ab".to_string())
            ]
        );
    }
}
