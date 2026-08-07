use std::time::{Duration, Instant};

use ratatui::text::Line;

use crate::presentation::{ActiveContent, View};

use crate::transcript::{
    AssistantSegment, SegmentKind,
    markdown::RenderedRow,
    model::TuiFormatter,
    row::TranscriptRow,
    slice_segments, think_to_text_newline,
    wrap::{TranscriptCommitBoundary, wrap_transcript_rows},
};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ACTIVE_STREAM_UNSTABLE_TAIL_ROWS: usize = 1;

#[derive(Debug, Clone)]
/// INTERNAL PRESENTATION MECHANICS.
///
/// Compatibility host for the current active renderer. Ordinary feature
/// content should implement `ActiveContent` and return a semantic `View`;
/// assistant streaming remains on its private spillable path.
pub(crate) enum ActivePaneState {
    WorkingSpinner {
        stream: ActiveStreamState,
        spinner_frame: usize,
    },
    AssistantStreaming {
        stream: ActiveStreamState,
    },
    Tool {
        tool_name: String,
        status: ToolActiveStatus,
        detail: Option<String>,
    },
    SlashMenu,
    FilePicker,
}

impl ActivePaneState {
    pub(crate) fn working_spinner() -> Self {
        Self::WorkingSpinner {
            stream: ActiveStreamState::new(),
            spinner_frame: 0,
        }
    }

    pub(crate) fn kind(&self) -> ActivePaneKind {
        match self {
            Self::WorkingSpinner { .. } => ActivePaneKind::WorkingSpinner,
            Self::AssistantStreaming { .. } => ActivePaneKind::AssistantStreaming,
            Self::Tool { .. } => ActivePaneKind::Tool,
            Self::SlashMenu => ActivePaneKind::SlashMenu,
            Self::FilePicker => ActivePaneKind::FilePicker,
        }
    }

    pub(crate) fn behavior(&self) -> ActiveBehavior {
        match self {
            Self::WorkingSpinner { .. } | Self::AssistantStreaming { .. } => {
                ActiveBehavior::SpillToTranscript
            }
            Self::Tool { .. } | Self::SlashMenu | Self::FilePicker => ActiveBehavior::OccludeOnly,
        }
    }

    pub(crate) fn is_spillable(&self) -> bool {
        self.behavior() == ActiveBehavior::SpillToTranscript
    }

    pub(crate) fn stream(&self) -> Option<&ActiveStreamState> {
        match self {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => {
                Some(stream)
            }
            Self::Tool { .. } | Self::SlashMenu | Self::FilePicker => None,
        }
    }

    pub(crate) fn stream_mut(&mut self) -> Option<&mut ActiveStreamState> {
        match self {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => {
                Some(stream)
            }
            Self::Tool { .. } | Self::SlashMenu | Self::FilePicker => None,
        }
    }

    pub(crate) fn ensure_stream_render_cache(&mut self, width: u16) {
        if let Some(stream) = self.stream_mut() {
            stream.ensure_render_cache(width);
        }
    }

    pub(crate) fn desired_height(&self) -> u16 {
        match self {
            Self::WorkingSpinner { .. } => 3,
            Self::AssistantStreaming { stream } => stream.desired_height(),
            Self::Tool { .. } | Self::SlashMenu | Self::FilePicker => 3,
        }
    }

    pub(crate) fn spill_overflow_rows(
        &mut self,
        visible_rows: usize,
    ) -> Option<Vec<AssistantSegment>> {
        let stream = self.stream_mut()?;
        stream.spill_overflow_rows(visible_rows)
    }

    pub(crate) fn tick(&mut self) {
        if let Self::WorkingSpinner { spinner_frame, .. } = self {
            *spinner_frame = spinner_frame.wrapping_add(1);
        }
    }

    pub(crate) fn spinner_frame(&self) -> &'static str {
        let frame = match self {
            Self::WorkingSpinner { spinner_frame, .. } => *spinner_frame,
            _ => 0,
        };
        SPINNER_FRAMES[frame % SPINNER_FRAMES.len()]
    }

    pub(crate) fn push_assistant_segment(&mut self, kind: SegmentKind, chunk: &str) {
        if chunk.is_empty() || !self.is_spillable() {
            return;
        }

        let current = std::mem::replace(
            self,
            Self::AssistantStreaming {
                stream: ActiveStreamState::new(),
            },
        );

        let mut stream = match current {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => stream,
            other => {
                *self = other;
                return;
            }
        };

        stream.push_delta(kind, chunk);
        *self = Self::AssistantStreaming { stream };
    }

    pub(crate) fn into_unfrozen_transcript_segments(self) -> Option<Vec<AssistantSegment>> {
        let stream = match self {
            Self::WorkingSpinner { stream, .. } | Self::AssistantStreaming { stream } => stream,
            Self::Tool { .. } | Self::SlashMenu | Self::FilePicker => return None,
        };

        let segments = stream.into_unfrozen_segments();
        if segments.is_empty() {
            None
        } else {
            Some(segments)
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ToolActiveStatus {
    WaitingForApproval { approval_id: u64 },
    Running,
}

/// INTERNAL PRESENTATION MECHANICS.
///
/// Source-backed streaming implementation retained separately from ordinary
/// `ActiveContent`; only this specialized path may expose stable spill state.
#[derive(Debug, Clone)]
pub(crate) struct ActiveStreamState {
    segments: Vec<AssistantSegment>,
    full_text: String,
    frozen_until: usize,
    revision: u64,
    render_cache: Option<ActiveStreamRenderCache>,
}

impl ActiveStreamState {
    pub(crate) fn new() -> Self {
        Self {
            segments: Vec::new(),
            full_text: String::new(),
            frozen_until: 0,
            revision: 0,
            render_cache: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn full_text(&self) -> &str {
        &self.full_text
    }

    #[cfg(test)]
    pub(crate) fn segments(&self) -> &[AssistantSegment] {
        &self.segments
    }

    #[cfg(test)]
    pub(crate) fn frozen_until(&self) -> usize {
        self.frozen_until
    }

    pub(crate) fn active_tail(&self) -> &str {
        &self.full_text[self.frozen_until..]
    }

    pub(crate) fn rendered_tail_rows(&self) -> Option<&[Line<'static>]> {
        self.render_cache
            .as_ref()
            .map(|cache| cache.body_rows.as_slice())
    }

    /// Appends a streamed chunk to the active stream. `full_text` (the flat byte
    /// source for freeze/spill math) and `segments` (the kind-annotated logical
    /// representation) are kept in sync: the chunk is appended to both, merging
    /// into the trailing segment when it is the same kind.
    pub(crate) fn push_delta(&mut self, kind: SegmentKind, chunk: &str) {
        if chunk.is_empty() {
            return;
        }

        // UX: the agent's answer starts on its own line after reasoning. This is
        // the single central rule (see think_to_text_newline) shared with the
        // transcript assembly path `push_segment`, so every code path that builds
        // assistant segments inserts exactly one newline between a thinking segment
        // and the answer text that follows it.
        let chunk = think_to_text_newline(&self.segments, kind, chunk);

        self.full_text.push_str(&chunk);
        match kind {
            SegmentKind::Text => {
                if let Some(AssistantSegment::Text(text)) = self.segments.last_mut() {
                    text.push_str(&chunk);
                } else {
                    self.segments
                        .push(AssistantSegment::Text(chunk.into_owned()));
                }
            }
            SegmentKind::Thinking => {
                if let Some(AssistantSegment::Thinking(text)) = self.segments.last_mut() {
                    text.push_str(&chunk);
                } else {
                    self.segments
                        .push(AssistantSegment::Thinking(chunk.into_owned()));
                }
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn advance_frozen_until(&mut self, mut next: usize) {
        next = next.min(self.full_text.len());
        while next > 0 && !self.full_text.is_char_boundary(next) {
            next -= 1;
        }

        if next > self.frozen_until {
            self.frozen_until = next;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn desired_height(&self) -> u16 {
        let body_rows = self
            .render_cache
            .as_ref()
            .map_or(1, |cache| cache.body_rows.len());

        // Active streaming has pane-level margins that are not part of the message body:
        // - top padding exists only before any text has spilled into the transcript;
        //   after spilling, the transcript's assistant top padding owns that gap.
        // - bottom padding is always reserved so the live pane does not crash into input.
        let total_rows = body_rows
            .saturating_add(self.top_padding_rows())
            .saturating_add(self.bottom_padding_rows());

        u16::try_from(total_rows).unwrap_or(u16::MAX).max(1)
    }

    fn spill_overflow_rows(&mut self, visible_rows: usize) -> Option<Vec<AssistantSegment>> {
        // Keep spill capacity in sync with ActiveView rendering. These reserved rows are
        // pane margins, not stream body rows, so they must not delay overflow/spilling.
        let reserved_rows = self
            .top_padding_rows()
            .saturating_add(self.bottom_padding_rows());
        let body_capacity = visible_rows.saturating_sub(reserved_rows).max(1);
        let cache = self.render_cache.as_ref()?;
        if cache.body_rows.len() <= body_capacity {
            return None;
        }

        let row_count = cache.body_rows.len();
        let stable_rows = row_count.saturating_sub(ACTIVE_STREAM_UNSTABLE_TAIL_ROWS);
        let desired_spill = row_count - body_capacity;
        let spill_rows = desired_spill.min(stable_rows);
        if spill_rows == 0 {
            return None;
        }

        let boundary = cache.row_end_boundaries[spill_rows - 1];
        let tail_bytes = self.boundary_to_tail_byte(boundary);
        if tail_bytes == 0 {
            return None;
        }

        let mut next_frozen = self.frozen_until + tail_bytes;
        if next_frozen < self.full_text.len() && self.full_text[next_frozen..].starts_with('\n') {
            next_frozen += '\n'.len_utf8();
        }

        let fragment = slice_segments(&self.segments, self.frozen_until, next_frozen);
        self.advance_frozen_until(next_frozen);
        Some(fragment)
    }

    fn ensure_render_cache(&mut self, width: u16) {
        let width = width.max(1);
        let key = (self.revision, width);
        if self
            .render_cache
            .as_ref()
            .is_some_and(|cache| cache.key == key)
        {
            return;
        }

        let tail_segments = slice_segments(&self.segments, self.frozen_until, usize::MAX);
        let rendered = TuiFormatter::default().format_assistant_body_meta(&tail_segments);
        let logical_rows: Vec<TranscriptRow> = rendered.iter().map(|rr| rr.row.clone()).collect();
        let wrapped =
            wrap_transcript_rows(width, &logical_rows, TranscriptCommitBoundary::default());
        self.render_cache = Some(ActiveStreamRenderCache {
            key,
            rows: rendered,
            body_rows: wrapped.rows,
            row_end_boundaries: wrapped.row_end_boundaries,
        });
    }

    pub(crate) fn top_padding_rows(&self) -> usize {
        // Before the first spill, there is no open assistant row in the transcript yet,
        // so the live pane must render the assistant message's leading gap itself. Once
        // anything has spilled, TranscriptState preserves that top padding instead.
        usize::from(!self.has_spilled_content())
    }

    pub(crate) fn bottom_padding_rows(&self) -> usize {
        // This is the active pane's margin above the input box. It deliberately lives in
        // the live pane rather than in streamed text, so it stays present even when the
        // stream does not end with a newline.
        1
    }

    fn has_spilled_content(&self) -> bool {
        self.frozen_until > 0
    }
}

impl ActiveStreamState {
    /// Maps a wrap-produced boundary back to a byte offset within the active tail
    /// (0-based from `frozen_until`). Unrestricted (1:1) rows are summed via their
    /// rendered span lengths; restricted rows — those whose `line` spans don't
    /// cover the full source (hidden markdown markers, or structural markers that
    /// hang in the gutter) — are always snapped to the end of the whole row
    /// (`content_len`), guaranteeing a partially-frozen markdown element never
    /// renders differently live vs. committed.
    fn boundary_to_tail_byte(&self, boundary: TranscriptCommitBoundary) -> usize {
        let Some(cache) = self.render_cache.as_ref() else {
            return 0;
        };
        let rows = &cache.rows;
        let mut byte = 0usize;

        for (index, rendered) in rows.iter().enumerate() {
            if index == boundary.logical_row {
                if rendered.restricted {
                    return byte + rendered.content_len;
                }
                let mut within_row = 0usize;
                for (span_index, span) in rendered.row.line.spans.iter().enumerate() {
                    if span_index == boundary.span_index {
                        within_row += boundary.byte_offset.min(span.content.len());
                        return byte + within_row;
                    }
                    within_row += span.content.len();
                }
                return byte + within_row;
            }
            byte += rendered.content_len + 1; // + the '\n' between logical rows
        }

        byte
    }

    fn into_unfrozen_segments(&self) -> Vec<AssistantSegment> {
        slice_segments(&self.segments, self.frozen_until, usize::MAX)
    }
}

impl ActiveContent for ActivePaneState {
    fn view(&self) -> View {
        match self {
            Self::WorkingSpinner { spinner_frame, .. } => View::column(
                vec![
                    View::spacer(1),
                    View::text(format!(
                        "{} Working",
                        SPINNER_FRAMES[*spinner_frame % SPINNER_FRAMES.len()]
                    )),
                    View::spacer(1),
                ],
                0,
            ),
            Self::AssistantStreaming { stream } => View::text(stream.active_tail()),
            Self::Tool {
                tool_name,
                status,
                detail,
            } => {
                let status = match status {
                    ToolActiveStatus::WaitingForApproval { .. } => "approval required",
                    ToolActiveStatus::Running => "running",
                };
                View::column(
                    vec![
                        View::spacer(1),
                        View::text(format!("tool {tool_name}: {status}")),
                        View::text(detail.as_deref().unwrap_or("")),
                    ],
                    0,
                )
            }
            Self::SlashMenu | Self::FilePicker => View::spacer(1),
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveStreamRenderCache {
    key: (u64, u16),
    rows: Vec<RenderedRow>,
    body_rows: Vec<Line<'static>>,
    row_end_boundaries: Vec<TranscriptCommitBoundary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivePaneKind {
    WorkingSpinner,
    AssistantStreaming,
    Tool,
    SlashMenu,
    FilePicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveBehavior {
    SpillToTranscript,
    OccludeOnly,
}

#[derive(Debug)]
pub(crate) struct ActiveTicker {
    interval: Duration,
    next_tick: Instant,
    animating: bool,
}

impl ActiveTicker {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_tick: Instant::now() + interval,
            animating: false,
        }
    }

    pub(crate) fn wait_timeout(
        &mut self,
        now: Instant,
        active: Option<&ActivePaneState>,
        idle_timeout: Duration,
    ) -> Duration {
        if !is_spinner_animating(active) {
            self.animating = false;
            self.next_tick = now + self.interval;
            return idle_timeout;
        }

        if !self.animating {
            self.animating = true;
            self.next_tick = now + self.interval;
        }

        self.next_tick.saturating_duration_since(now)
    }

    pub(crate) fn tick_if_due(
        &mut self,
        now: Instant,
        active: Option<&mut ActivePaneState>,
    ) -> bool {
        let Some(active) = active else {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        };

        if active.kind() != ActivePaneKind::WorkingSpinner {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        }

        if now < self.next_tick {
            return false;
        }

        active.tick();
        self.next_tick = now + self.interval;
        true
    }
}

fn is_spinner_animating(active: Option<&ActivePaneState>) -> bool {
    matches!(
        active.map(ActivePaneState::kind),
        Some(ActivePaneKind::WorkingSpinner)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{AssistantSegment, SegmentKind, slice_segments};

    fn concat_text(segments: &[AssistantSegment]) -> String {
        segments
            .iter()
            .map(|segment| segment.text().to_string())
            .collect()
    }

    #[test]
    fn push_delta_merges_contiguous_same_kind_segments() {
        let mut thinking = ActiveStreamState::new();
        thinking.push_delta(SegmentKind::Thinking, "a");
        thinking.push_delta(SegmentKind::Thinking, "b");
        assert_eq!(
            thinking.segments(),
            &[AssistantSegment::Thinking("ab".to_string())]
        );
        assert_eq!(thinking.full_text(), "ab");

        let mut text = ActiveStreamState::new();
        text.push_delta(SegmentKind::Text, "p");
        text.push_delta(SegmentKind::Text, "q");
        assert_eq!(text.segments(), &[AssistantSegment::Text("pq".to_string())]);
        assert_eq!(text.full_text(), "pq");
    }

    #[test]
    fn push_delta_inserts_single_newline_between_thinking_and_text() {
        let mut stream = ActiveStreamState::new();
        stream.push_delta(SegmentKind::Thinking, "a");
        stream.push_delta(SegmentKind::Thinking, "b");
        stream.push_delta(SegmentKind::Text, "c");
        stream.push_delta(SegmentKind::Thinking, "d");

        // The central think-to-text rule inserts an empty line (two \n) so the
        // rendered transcript gets a real blank row between thinking and answer.
        assert_eq!(
            stream.segments(),
            &[
                AssistantSegment::Thinking("ab".to_string()),
                AssistantSegment::Text("\n\nc".to_string()),
                AssistantSegment::Thinking("d".to_string()),
            ]
        );
        assert_eq!(stream.full_text(), "ab\n\ncd");

        // No extra newline when the thinking segment already ended with one.
        let mut already_ended = ActiveStreamState::new();
        already_ended.push_delta(SegmentKind::Thinking, "x\n");
        already_ended.push_delta(SegmentKind::Text, "y");
        assert_eq!(already_ended.full_text(), "x\ny");
    }

    #[test]
    fn active_tail_reflects_frozen_until() {
        let mut stream = ActiveStreamState::new();
        stream.push_delta(SegmentKind::Text, "hello");
        stream.advance_frozen_until(2);
        assert_eq!(stream.active_tail(), "llo");
    }

    #[test]
    fn advance_frozen_until_clamps_to_utf8_char_boundary() {
        let mut stream = ActiveStreamState::new();
        stream.push_delta(SegmentKind::Text, "héllo");
        stream.advance_frozen_until(2); // 2 is mid 'é' (bytes 1..=2) -> clamps down to 1
        assert_eq!(stream.frozen_until(), 1);
    }

    #[test]
    fn into_unfrozen_returns_only_non_frozen_segments() {
        let mut stream = ActiveStreamState::new();
        stream.push_delta(SegmentKind::Thinking, "ab");
        stream.push_delta(SegmentKind::Text, "cd"); // injected blank line -> "ab\n\ncd"
        stream.advance_frozen_until(2);

        assert_eq!(
            stream.into_unfrozen_segments(),
            vec![AssistantSegment::Text("\n\ncd".to_string())]
        );
    }

    #[test]
    fn boundary_to_tail_byte_is_span_aware_on_mixed_lines() {
        let mut stream = ActiveStreamState::new();
        // A Text->Thinking transition has no injected newline, so the thinking span
        // lands on the same logical row as the preceding answer text. This is the
        // case where span-aware byte accounting matters.
        stream.push_delta(SegmentKind::Text, "A");
        stream.push_delta(SegmentKind::Thinking, "B");
        stream.ensure_render_cache(10); // wide enough to keep one visual row

        let cache = stream.render_cache.as_ref().expect("render cache");
        assert_eq!(cache.rows.len(), 1);
        assert_eq!(cache.rows[0].row.line.spans.len(), 2);
        // Spans: ["A"(text), "B"(thinking)] -> flat bytes A=0, B=1.
        let end_of_a = TranscriptCommitBoundary {
            logical_row: 0,
            span_index: 0,
            byte_offset: 1,
        };
        assert_eq!(stream.boundary_to_tail_byte(end_of_a), 1);

        let start_of_b = TranscriptCommitBoundary {
            logical_row: 0,
            span_index: 1,
            byte_offset: 0,
        };
        assert_eq!(stream.boundary_to_tail_byte(start_of_b), 1);

        let end_of_b = TranscriptCommitBoundary {
            logical_row: 0,
            span_index: 1,
            byte_offset: 1,
        };
        assert_eq!(stream.boundary_to_tail_byte(end_of_b), 2);
    }

    #[test]
    fn spill_never_loses_or_duplicates_text_across_segments() {
        let mut stream = ActiveStreamState::new();
        let thinking = "think one\nthink two\n";
        let text = "answer one\nanswer two\n";
        stream.push_delta(SegmentKind::Thinking, thinking);
        stream.push_delta(SegmentKind::Text, text);

        let width = 5u16;
        let visible_rows = 2usize;

        let mut spilled: Vec<AssistantSegment> = Vec::new();
        for _ in 0..50 {
            stream.ensure_render_cache(width);
            match stream.spill_overflow_rows(visible_rows) {
                Some(fragment) => spilled.extend(fragment),
                None => break,
            }
        }

        let remaining = stream.into_unfrozen_segments();
        let all = format!("{}{}", concat_text(&spilled), concat_text(&remaining));
        let full = format!("{thinking}{text}");
        assert_eq!(
            all, full,
            "segment-aware spill must round-trip the full stream"
        );
    }

    #[test]
    fn spill_round_trips_markdown_with_restricted_rows() {
        // Markdown with hidden markers (bold), a list bullet, a header, and plain
        // text. Restricted rows (bold/list) must be frozen whole-line so the
        // committed transcript never shows half-rendered markup; the flat source
        // must still round-trip with zero loss/duplication.
        let mut stream = ActiveStreamState::new();
        let markdown = "# Head\n- item\n**bold** text\nplain line here\n";
        stream.push_delta(SegmentKind::Text, markdown);

        let width = 8u16;
        let visible_rows = 1usize; // force aggressive spilling

        let mut spilled: Vec<AssistantSegment> = Vec::new();
        for _ in 0..200 {
            stream.ensure_render_cache(width);
            match stream.spill_overflow_rows(visible_rows) {
                Some(fragment) => spilled.extend(fragment),
                None => break,
            }
        }

        let remaining = stream.into_unfrozen_segments();
        let all = format!("{}{}", concat_text(&spilled), concat_text(&remaining));
        assert_eq!(
            all, markdown,
            "markdown spill must round-trip the flat source exactly"
        );
    }

    #[test]
    fn spill_round_trips_markdown_with_multi_byte_chars() {
        // Regression: a multi-byte glyph (e.g. an em dash) inside / after hidden
        // markdown markers must never let a spill boundary land mid-char (a
        // `full_text[next_frozen..]` panic). The flat source must round-trip
        // byte-for-byte.
        let mut stream = ActiveStreamState::new();
        let text = "1. a long item — with an em dash — that wraps\n2. second — line\n";
        for width in [4u16, 6, 8, 10, 12] {
            for visible in [1usize, 2] {
                let mut stream = ActiveStreamState::new();
                stream.push_delta(SegmentKind::Text, text);
                let mut spilled: Vec<AssistantSegment> = Vec::new();
                for _ in 0..1000 {
                    stream.ensure_render_cache(width);
                    match stream.spill_overflow_rows(visible) {
                        Some(frag) => spilled.extend(frag),
                        None => break,
                    }
                }
                let remaining = stream.into_unfrozen_segments();
                let all = format!("{}{}", concat_text(&spilled), concat_text(&remaining));
                assert_eq!(
                    all, text,
                    "width {width} visible {visible}: ordered multi-byte spill round-trip"
                );
            }
        }
    }

    #[test]
    fn slice_segments_handles_mid_segment_split() {
        let segments = vec![
            AssistantSegment::Thinking("abc".to_string()),
            AssistantSegment::Text("def".to_string()),
        ];
        // bytes: a=0 b=1 c=2 d=3 e=4 f=5 -> [1,4) = "bc" (thinking) + "d" (text)
        assert_eq!(
            slice_segments(&segments, 1, 4),
            vec![
                AssistantSegment::Thinking("bc".to_string()),
                AssistantSegment::Text("d".to_string()),
            ]
        );
    }
}
