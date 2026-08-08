use std::{borrow::Cow, collections::HashMap, ops::Range};

use ratatui::{style::Style, text::Line};

use crate::{
    presentation::{
        ColorSpec, Decoration, FlowBoundary, Insets, LeadingBoundaryState, ResidentStreamHandoff,
        StreamOffset, ThemeKey, View, WidthRule, internal::compile_view,
    },
    theme,
    tools::{ToolCallRenderInput, ToolOutcome, ToolRendererRegistry, ToolResultRenderInput},
    transcript::{
        markdown,
        markdown::{RenderedRow, assistant_document_view, parse_assistant},
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResidentStreamTail {
    /// First source byte not already owned by native history.
    pub(crate) source_base: StreamOffset,
    /// Final source boundary of the sealed stream.
    pub(crate) source_end: StreamOffset,
    /// Static semantic presentation of only `source_base..source_end`.
    pub(crate) view: View,
    /// Flow boundary still owned by the resident tail.
    pub(crate) boundary: FlowBoundary,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PresentationOverride {
    Hosted,
    Resident(ResidentStreamTail),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentationOwnership {
    Transcript,
    Hosted,
    ResidentStream,
    NativeHistory,
}

#[derive(Debug, Clone)]
pub(crate) enum TranscriptPresentation {
    View(View),
    ResidentStream,

    /// INTERNAL ASSISTANT STREAM SEMANTICS.
    ///
    /// Source-backed compatibility representation used only while an assistant unit
    /// requires the legacy source/commit mapping.
    SourceBackedAssistantRows(Vec<TranscriptRow>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryLifecycle {
    Open,
    Sealed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryCommitMode {
    Sealed,
    SourceBackedAppendOnly,
    Blocked,
}

/// The identity of a presentation unit. Unit IDs are derived from (and share
/// the namespace of) the first canonical source item's stable [`TranscriptId`],
/// so unit identity survives presentation rebuilds.
pub(crate) type TranscriptUnitId = TranscriptId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostedUnitRegistration {
    pub(crate) id: TranscriptUnitId,
    pub(crate) leading_boundary: LeadingBoundaryState,
}

/// A presentation unit: the owned, structural object the transcript is assembled
/// from and that native history consumes. One or more canonical items may fold
/// into a single unit (e.g. a maximal run of consecutive user messages becomes
/// one bubble). The unit owns its inside; transcript flow owns the gap between
/// units.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptUnit {
    pub(crate) id: TranscriptUnitId,
    pub(crate) source_ids: Vec<TranscriptId>,
    pub(crate) presentation: TranscriptPresentation,
    pub(crate) ownership: PresentationOwnership,
    pub(crate) boundary: FlowBoundary,
    pub(crate) truncation: Truncation,
    pub(crate) lifecycle: EntryLifecycle,
    commit_mode: EntryCommitMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Truncation {
    None,
    Call,
    Result,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TuiFormatter {
    tool_renderers: ToolRendererRegistry,
    /// Debug aid: render tool-call argument previews under the call header.
    /// Defaults off via `Default`; the normal UI keeps calls compact.
    show_arg_preview: bool,
}

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> TranscriptPresentation {
        match item {
            // User messages are semantic bubbles; a single message is a one-
            // message batch. (The assembler builds maximal runs itself; this arm
            // only serves the formatter's direct contract.)
            TimelineItem::UserMessage { text } => {
                TranscriptPresentation::View(Self::user_batch_view(std::slice::from_ref(text)))
            }
            TimelineItem::AssistantMessage { segments } => {
                let doc = parse_assistant(segments);
                TranscriptPresentation::View(assistant_document_view(&doc, false))
            }
            TimelineItem::ErrorMessage { text } => {
                TranscriptPresentation::View(self.format_error_message(text, false))
            }
            TimelineItem::ToolCall {
                tool_name,
                arguments,
                status,
                ..
            } => TranscriptPresentation::View(self.format_tool_call(tool_name, arguments, *status)),
            TimelineItem::ToolResult {
                tool_name,
                text,
                details,
                is_error,
                collapsed,
                ..
            } => TranscriptPresentation::View(
                self.format_tool_result(tool_name, text, details, *is_error, *collapsed),
            ),
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

    /// Formats `segments` into logical rows, including the legacy rendering
    /// metadata used by compatibility callers.
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

    /// Semantic user bubble: one structural Box owning the full-width background
    /// and the intrinsic vertical padding. A run of consecutive user messages
    /// feeds the inner `Column`, so the whole group is one visual object.
    ///
    /// The legacy user renderer's oracle geometry is: full-width background,
    /// content at column 0 (no horizontal inset), one row of vertical padding on
    /// top and bottom. That is what this Box reproduces exactly — padding is
    /// structural (inside the bubble); cross-unit spacing stays with flow.
    fn user_batch_view(messages: &[String]) -> View {
        View::box_(
            View::column(
                messages
                    .iter()
                    .map(|text| View::text(text.clone()).width(WidthRule::Fill))
                    .collect(),
                0,
            )
            .width(WidthRule::Fill),
            Decoration::background(ColorSpec::Theme(ThemeKey::from("surface.user"))).padding(
                Insets {
                    top: 1,
                    right: 0,
                    bottom: 1,
                    left: 0,
                },
            ),
        )
        .width(WidthRule::Fill)
    }

    fn format_error_message(&self, text: &str, leading_spacer: bool) -> View {
        let body = View::text(text)
            .width(crate::presentation::WidthRule::Fill)
            .style(crate::presentation::StyleSpec {
                foreground: Some(crate::presentation::ColorSpec::Theme(
                    crate::presentation::ThemeKey::from("text.error"),
                )),
                ..crate::presentation::StyleSpec::default()
            });
        if leading_spacer {
            View::column(vec![View::spacer(1), body], 0)
        } else {
            body
        }
    }

    fn format_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        status: ToolTimelineStatus,
    ) -> View {
        self.tool_renderers.render_call(ToolCallRenderInput {
            tool_name,
            arguments,
            status,
            show_arg_preview: self.show_arg_preview,
        })
    }

    fn format_tool_result(
        &self,
        tool_name: &str,
        text: &str,
        details: &serde_json::Value,
        is_error: bool,
        _collapsed: bool,
    ) -> View {
        self.tool_renderers.render_result(ToolResultRenderInput {
            tool_name,
            text,
            details,
            outcome: if is_error {
                ToolOutcome::Error
            } else {
                ToolOutcome::Success
            },
        })
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

#[derive(Debug, Clone)]
pub(crate) struct TranscriptState {
    canonical_items: Vec<TimelineItem>,
    entry_ids: Vec<TranscriptId>,
    next_id: u64,
    presentation_cache: Vec<TranscriptUnit>,
    rendered_rows_cache: Option<TranscriptRenderCache>,
    commit_state: TranscriptCommitState,
    formatter: TuiFormatter,
    assistant_stream_open: bool,
    overrides: HashMap<TranscriptUnitId, PresentationOverride>,
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            canonical_items: Vec::new(),
            entry_ids: Vec::new(),
            next_id: 0,
            presentation_cache: Vec::new(),
            rendered_rows_cache: None,
            commit_state: TranscriptCommitState::default(),
            formatter: TuiFormatter::default(),
            assistant_stream_open: false,
            overrides: HashMap::new(),
        }
    }
}

impl TranscriptState {
    pub(crate) fn has_conversation_content(&self) -> bool {
        !self.presentation_cache.is_empty()
    }

    pub(crate) fn push_item(&mut self, item: TimelineItem) {
        self.assistant_stream_open = false;
        // If a user message extends a trailing user run, the merged batch must not
        // have begun entering native history. In normal flow consecutive user
        // messages are assembled before a frame commits the first part; this guard
        // catches an illegal late extension.
        if let TimelineItem::UserMessage { .. } = &item
            && self
                .canonical_items
                .last()
                .is_some_and(|last| matches!(last, TimelineItem::UserMessage { .. }))
        {
            if let Some(&last_id) = self.entry_ids.last() {
                self.assert_unit_extendable(last_id);
            }
        }
        self.canonical_items.push(item);
        let id = self.allocate_id();
        self.entry_ids.push(id);
        self.rebuild_presentation_cache();
    }

    /// A grouped unit (user batch) may only grow while its presentation has not
    /// begun entering native history. Arbitrary late extension would mutate a
    /// frozen/committed bubble.
    fn assert_unit_extendable(&self, source_id: TranscriptId) {
        let Some(unit_index) = self.unit_index_for_source(source_id) else {
            return;
        };
        let id = self.presentation_cache[unit_index].id;
        debug_assert!(
            unit_index >= self.commit_state.completed_prefix
                && !self
                    .commit_state
                    .partial
                    .as_ref()
                    .is_some_and(|partial| partial.entry_id() == id),
            "attempted to extend user batch {id:?} after native history commitment"
        );
    }

    pub(crate) fn begin_hosted_assistant_unit(&mut self) -> HostedUnitRegistration {
        let id = self.allocate_id();
        let leading_boundary = if self.canonical_items.is_empty() {
            LeadingBoundaryState::None
        } else {
            LeadingBoundaryState::Pending
        };

        self.canonical_items.push(TimelineItem::AssistantMessage {
            segments: Vec::new(),
        });
        self.entry_ids.push(id);
        assert!(
            self.overrides
                .insert(id, PresentationOverride::Hosted)
                .is_none(),
            "a hosted transcript unit cannot be registered twice"
        );
        self.assistant_stream_open = true;
        self.rebuild_presentation_cache();

        HostedUnitRegistration {
            id,
            leading_boundary,
        }
    }

    fn allocate_id(&mut self) -> TranscriptId {
        let id = TranscriptId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// Maps a stable canonical source id to the presentation unit that owns it.
    /// A grouped unit (e.g. a user batch) owns several source ids, so this is
    /// many-to-one. Performance is irrelevant here; no lookup table is needed.
    fn unit_index_for_source(&self, source_id: TranscriptId) -> Option<usize> {
        self.presentation_cache
            .iter()
            .position(|unit| unit.source_ids.contains(&source_id))
    }

    /// Rejects mutation after a unit has begun entering native history.
    fn assert_source_mutable(&self, source_id: TranscriptId) {
        let Some(unit_index) = self.unit_index_for_source(source_id) else {
            return;
        };
        let id = self.presentation_cache[unit_index].id;
        debug_assert!(
            unit_index >= self.commit_state.completed_prefix,
            "attempted to mutate fully committed transcript unit {id:?}"
        );
        if let Some(partial) = &self.commit_state.partial {
            debug_assert_ne!(
                partial.entry_id(),
                id,
                "attempted to mutate partially committed transcript unit {id:?}"
            );
        }
    }

    /// The dedicated append check for the source-backed assistant stream: appending
    /// more source after a committed prefix is legal, but only if that entry retains
    /// its source-backed cursor. Arbitrary mutation is still rejected by
    /// [`Self::assert_source_mutable`].
    fn assert_source_stream_appendable(&self, source_id: TranscriptId) {
        let Some(unit_index) = self.unit_index_for_source(source_id) else {
            return;
        };
        let id = self.presentation_cache[unit_index].id;
        debug_assert!(
            unit_index >= self.commit_state.completed_prefix,
            "attempted to append after a completed transcript prefix"
        );
        if let Some(partial) = &self.commit_state.partial {
            debug_assert!(
                matches!(partial, PartialCommit::Legacy { entry_id, .. } if *entry_id == id),
                "committed open stream must retain a source-backed cursor"
            );
        }
    }

    pub(crate) fn upsert_tool_call(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ToolTimelineStatus,
    ) {
        self.assistant_stream_open = false;
        let existing_index = self.canonical_items.iter().position(|item| {
            matches!(
                item,
                TimelineItem::ToolCall { tool_call_id: existing, .. } if existing == &tool_call_id
            )
        });
        if let Some(index) = existing_index {
            self.assert_source_mutable(self.entry_ids[index]);
        }
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
            let id = self.allocate_id();
            self.entry_ids.push(id);
        }
        self.rebuild_presentation_cache();
    }

    pub(crate) fn update_tool_call_status(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        status: ToolTimelineStatus,
    ) {
        let existing_index = self.canonical_items.iter().position(|item| {
            matches!(
                item,
                TimelineItem::ToolCall { tool_call_id: existing, .. } if existing == &tool_call_id
            )
        });
        if let Some(index) = existing_index {
            self.assert_source_mutable(self.entry_ids[index]);
        }
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
            self.rebuild_presentation_cache();
        }
    }

    pub(crate) fn finish_tool_call(&mut self, tool_call_id: &str, is_error: bool) {
        let status = if is_error {
            ToolTimelineStatus::Failed
        } else {
            ToolTimelineStatus::Finished
        };
        let existing_index = self.canonical_items.iter().position(|item| {
            matches!(
                item,
                TimelineItem::ToolCall { tool_call_id: existing, .. } if existing == tool_call_id
            )
        });
        if let Some(index) = existing_index {
            self.assert_source_mutable(self.entry_ids[index]);
        }
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
            self.rebuild_presentation_cache();
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
        let id = self.allocate_id();
        self.entry_ids.push(id);
        self.rebuild_presentation_cache();
    }

    pub(crate) fn append_assistant_fragment(&mut self, segments: Vec<AssistantSegment>) {
        self.append_assistant_segments(segments, false);
    }

    pub(crate) fn seal_hosted_assistant_unit(
        &mut self,
        unit_id: TranscriptUnitId,
        incoming: Vec<AssistantSegment>,
    ) {
        let Some(index) = self.entry_ids.iter().position(|id| *id == unit_id) else {
            panic!("hosted stream unit must be registered at birth");
        };
        let mut segments = Vec::new();
        for segment in incoming {
            push_segment(&mut segments, segment);
        }
        match &mut self.canonical_items[index] {
            TimelineItem::AssistantMessage { segments: target } => *target = segments,
            _ => panic!("hosted stream unit must point to an assistant message"),
        }
        self.assistant_stream_open = false;
        assert!(
            matches!(
                self.overrides.get(&unit_id),
                Some(PresentationOverride::Hosted)
            ),
            "only a hosted unit can be sealed"
        );
        self.rebuild_presentation_cache();
        self.invalidate_render_cache();
    }

    pub(crate) fn hosted_unit_is_history_head(&self, unit_id: TranscriptUnitId) -> bool {
        let Some(index) = self
            .presentation_cache
            .iter()
            .position(|unit| unit.id == unit_id)
        else {
            return false;
        };

        matches!(
            self.overrides.get(&unit_id),
            Some(PresentationOverride::Hosted)
        ) && index == self.commit_state.completed_prefix
            && self.commit_state.partial.is_none()
    }

    pub(crate) fn hosted_unit_is_history_tail(&self, unit_id: TranscriptUnitId) -> bool {
        self.presentation_cache.last().is_some_and(|unit| {
            unit.id == unit_id
                && matches!(
                    self.overrides.get(&unit_id),
                    Some(PresentationOverride::Hosted)
                )
        })
    }

    pub(crate) fn adopt_resident_stream(&mut self, handoff: ResidentStreamHandoff) {
        assert!(
            handoff.source_base <= handoff.source_end,
            "resident source range must be ordered"
        );
        let unit_index = self
            .presentation_cache
            .iter()
            .position(|unit| unit.id == handoff.unit_id)
            .expect("resident handoff unit must exist");
        let unit = &self.presentation_cache[unit_index];
        assert!(
            matches!(&unit.ownership, PresentationOwnership::Hosted),
            "resident handoff must consume a hosted unit"
        );
        assert!(
            !self.assistant_stream_open,
            "resident handoff requires a sealed canonical assistant"
        );
        let source_id = *unit
            .source_ids
            .first()
            .expect("resident unit must own a canonical source");
        let source_index = self
            .entry_ids
            .iter()
            .position(|id| *id == source_id)
            .expect("resident source must exist in canonical history");
        assert!(
            matches!(
                self.canonical_items.get(source_index),
                Some(TimelineItem::AssistantMessage { .. })
            ),
            "resident handoff must target an assistant item"
        );

        let boundary = match handoff.leading_boundary {
            LeadingBoundaryState::Pending => unit.boundary,
            LeadingBoundaryState::Committed | LeadingBoundaryState::None => {
                FlowBoundary::AttachToPrevious
            }
        };
        self.overrides.insert(
            handoff.unit_id,
            PresentationOverride::Resident(ResidentStreamTail {
                source_base: handoff.source_base,
                source_end: handoff.source_end,
                view: handoff.view,
                boundary,
            }),
        );
        self.rebuild_presentation_cache();
        self.invalidate_render_cache();
    }

    pub(crate) fn finish_stream_unit(&mut self, unit_id: TranscriptUnitId) {
        let unit_index = self
            .presentation_cache
            .iter()
            .position(|unit| unit.id == unit_id)
            .expect("hosted stream unit must exist before retirement");
        assert!(
            matches!(
                self.overrides.get(&unit_id),
                Some(PresentationOverride::Hosted)
            ),
            "stream unit must be hosted before native retirement"
        );
        assert_eq!(
            unit_index, self.commit_state.completed_prefix,
            "hosted unit cannot retire ahead of global history frontier"
        );
        assert!(
            !self.assistant_stream_open,
            "hosted unit must be semantically sealed before native retirement"
        );
        assert!(
            self.commit_state.partial.is_none(),
            "hosted unit cannot retire while an earlier unit is partial"
        );

        self.overrides.remove(&unit_id);
        self.commit_state.completed_prefix = unit_index.saturating_add(1);
        self.rebuild_presentation_cache();
        self.invalidate_render_cache();
    }

    pub(crate) fn append_assistant_stream_fragment(&mut self, segments: Vec<AssistantSegment>) {
        self.append_assistant_segments(segments, true);
    }

    pub(crate) fn finish_assistant_stream(&mut self) {
        if !self.assistant_stream_open {
            return;
        }
        self.assistant_stream_open = false;
        self.rebuild_presentation_cache();
    }

    #[cfg(test)]
    pub(crate) fn test_items(&self) -> &[TimelineItem] {
        &self.canonical_items
    }

    #[cfg(test)]
    pub(crate) fn test_entry_ids(&self) -> &[TranscriptId] {
        &self.entry_ids
    }

    fn append_assistant_segments(&mut self, incoming: Vec<AssistantSegment>, stream_open: bool) {
        if incoming.is_empty() {
            return;
        }

        let last_index = self.canonical_items.len().checked_sub(1);
        if let Some(index) = last_index
            && matches!(
                self.canonical_items[index],
                TimelineItem::AssistantMessage { .. }
            )
        {
            let source_id = self.entry_ids[index];
            if self.assistant_stream_open || self.commit_state.partial.is_some() {
                self.assert_source_stream_appendable(source_id);
            } else {
                self.assert_source_mutable(source_id);
            }
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
                let id = self.allocate_id();
                self.entry_ids.push(id);
            }
        }
        self.assistant_stream_open = stream_open;
        self.rebuild_presentation_cache();
    }

    fn ownership_for(&self, unit_id: TranscriptUnitId, unit_index: usize) -> PresentationOwnership {
        if unit_index < self.commit_state.completed_prefix {
            return PresentationOwnership::NativeHistory;
        }
        match self.overrides.get(&unit_id) {
            Some(PresentationOverride::Hosted) => PresentationOwnership::Hosted,
            Some(PresentationOverride::Resident(_)) => PresentationOwnership::ResidentStream,
            None => PresentationOwnership::Transcript,
        }
    }

    fn rebuild_presentation_cache(&mut self) {
        self.presentation_cache.clear();
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

        let mut index = 0usize;
        while index < self.canonical_items.len() {
            // Build a maximal consecutive user run as ONE structural unit. Its id
            // is the first source id; it owns every message in the run.
            if matches!(
                self.canonical_items[index],
                TimelineItem::UserMessage { .. }
            ) {
                let start = index;
                let mut messages = Vec::new();
                while index < self.canonical_items.len()
                    && matches!(
                        self.canonical_items[index],
                        TimelineItem::UserMessage { .. }
                    )
                {
                    if let TimelineItem::UserMessage { text } = &self.canonical_items[index] {
                        messages.push(text.clone());
                    }
                    index += 1;
                }
                self.presentation_cache.push(TranscriptUnit {
                    id: self.entry_ids[start],
                    source_ids: self.entry_ids[start..index].to_vec(),
                    presentation: TranscriptPresentation::View(TuiFormatter::user_batch_view(
                        &messages,
                    )),
                    ownership: PresentationOwnership::Transcript,
                    boundary: FlowBoundary::Default,
                    truncation: Truncation::None,
                    lifecycle: EntryLifecycle::Sealed,
                    commit_mode: EntryCommitMode::Sealed,
                });
                continue;
            }

            let item = &self.canonical_items[index];
            let is_open = open_assistant_idx == Some(index);
            let source_id = self.entry_ids[index];
            let presentation_unit_index = self.presentation_cache.len();
            let ownership = self.ownership_for(source_id, presentation_unit_index);
            let source_backed_started = self.has_source_backed_partial(source_id);
            let presentation = match &ownership {
                PresentationOwnership::ResidentStream => TranscriptPresentation::ResidentStream,
                PresentationOwnership::Hosted | PresentationOwnership::NativeHistory => {
                    TranscriptPresentation::View(View::column(Vec::new(), 0))
                }
                PresentationOwnership::Transcript => match item {
                    TimelineItem::AssistantMessage { segments } => {
                        if is_open {
                            let mut rows = self.formatter.format_assistant_message(segments, false);
                            // INTERNAL ASSISTANT STREAM SEMANTICS.
                            // The live pane owns the moving bottom margin while this
                            // presentation is open.
                            drop_trailing_empty_row(&mut rows);
                            TranscriptPresentation::SourceBackedAssistantRows(rows)
                        } else if source_backed_started {
                            let rows = self.formatter.format_assistant_message(segments, true);
                            TranscriptPresentation::SourceBackedAssistantRows(rows)
                        } else {
                            let doc = parse_assistant(segments);
                            let view = assistant_document_view(&doc, index == 0);
                            TranscriptPresentation::View(view)
                        }
                    }
                    TimelineItem::ErrorMessage { text } => TranscriptPresentation::View(
                        self.formatter.format_error_message(text, index == 0),
                    ),
                    _ => self.formatter.format(item),
                },
            };

            let boundary = if index > 0
                && matches!(
                    (&self.canonical_items[index - 1], item),
                    (
                        TimelineItem::ToolCall {
                            tool_call_id: previous,
                            ..
                        },
                        TimelineItem::ToolResult {
                            tool_call_id: current,
                            ..
                        }
                    ) if previous == current
                ) {
                FlowBoundary::AttachToPrevious
            } else {
                FlowBoundary::Default
            };

            let truncation = match item {
                TimelineItem::ToolCall { .. } => Truncation::Call,
                TimelineItem::ToolResult {
                    collapsed: true, ..
                } => Truncation::Result,
                _ => Truncation::None,
            };
            let lifecycle = match &ownership {
                PresentationOwnership::Hosted if is_open => EntryLifecycle::Open,
                PresentationOwnership::Hosted
                | PresentationOwnership::ResidentStream
                | PresentationOwnership::NativeHistory => EntryLifecycle::Sealed,
                PresentationOwnership::Transcript => {
                    entry_lifecycle(item, open_assistant_idx == Some(index))
                }
            };
            let commit_mode = match &ownership {
                PresentationOwnership::Hosted => EntryCommitMode::Blocked,
                PresentationOwnership::ResidentStream
                | PresentationOwnership::NativeHistory
                | PresentationOwnership::Transcript => {
                    entry_commit_mode(item, open_assistant_idx == Some(index))
                }
            };
            let boundary = match self.overrides.get(&source_id) {
                Some(PresentationOverride::Resident(tail)) => tail.boundary,
                _ => boundary,
            };
            self.presentation_cache.push(TranscriptUnit {
                id: self.entry_ids[index],
                source_ids: vec![self.entry_ids[index]],
                presentation,
                ownership,
                boundary,
                truncation,
                lifecycle,
                commit_mode,
            });
            index += 1;
        }
        self.invalidate_render_cache();
    }

    fn has_source_backed_partial(&self, unit_id: TranscriptUnitId) -> bool {
        matches!(
            self.commit_state.partial.as_ref(),
            Some(PartialCommit::Legacy { entry_id, .. }) if *entry_id == unit_id
        )
    }

    fn invalidate_render_cache(&mut self) {
        if let Some(cache) = self.rendered_rows_cache.as_mut() {
            // Keep committed_rows while invalidating the width-specific suffix.
            // Native history is immutable; only this suffix is rebuilt.
            cache.width = 0;
        }
    }

    /// Width-aware bridge for mixed legacy/semantic entries.
    ///
    /// Every range owns its parent separator. Semantic ranges can therefore be
    /// frozen as a complete physical unit when native history starts consuming
    /// them; no later resize needs to skip a stale global row count.
    fn mixed_render(&self, width: u16) -> TranscriptRenderCache {
        let mut rows = Vec::new();
        let mut row_end_boundaries = Vec::new();
        let mut ranges = Vec::new();
        let mut previous_legacy_range: Option<usize> = None;
        let start_unit = self.commit_state.completed_prefix;
        let partial = self.commit_state.partial.as_ref();

        for (unit_index, unit) in self.presentation_cache.iter().enumerate().skip(start_unit) {
            let partial_for_unit = (unit_index == start_unit).then(|| partial);
            let semantic_frozen = matches!(
                partial_for_unit.flatten(),
                Some(PartialCommit::Semantic { entry_id, .. }) if *entry_id == unit.id
            );
            let legacy_gap_committed = matches!(
                partial_for_unit.flatten(),
                Some(PartialCommit::Legacy {
                    entry_id,
                    leading_gap_committed: true,
                    ..
                }) if *entry_id == unit.id
            );

            // `Default` owns its leading flow gap; `AttachToPrevious` means only
            // "parent gap = 0" (no fusion, no row surgery). A frozen semantic unit
            // already contains its gap, so it must not be emitted twice. Assembly
            // happens before the range starts so the range owns exactly the rows that
            // follow the preceding unit.
            if unit.boundary == FlowBoundary::Default {
                if let Some(previous_range) = previous_legacy_range {
                    // The only remaining legacy family is the assistant. Remove its
                    // formatter-owned trailing plain row before the parent emits its
                    // canonical flow gap.
                    trim_plain_trailing_physical(
                        &mut rows,
                        &mut row_end_boundaries,
                        &mut ranges,
                        previous_range,
                    );
                }
            }

            let start = rows.len();
            let hosted_or_native_history = matches!(
                &unit.ownership,
                PresentationOwnership::Hosted | PresentationOwnership::NativeHistory
            );
            if unit.boundary == FlowBoundary::Default
                && unit_index > 0
                && !semantic_frozen
                && !legacy_gap_committed
                && !hosted_or_native_history
            {
                rows.push(Line::from(""));
                row_end_boundaries.push(None);
            }
            let leading_flow_rows = rows.len().saturating_sub(start);
            let content_start = rows.len();
            let mut entry_committable_prefix = 0;
            match &unit.presentation {
                TranscriptPresentation::SourceBackedAssistantRows(logical_rows) => {
                    let mut logical_rows = logical_rows.clone();
                    if unit_index > 0 {
                        trim_plain_leading_legacy(&mut logical_rows);
                    }

                    let boundary = match partial_for_unit.flatten() {
                        Some(PartialCommit::Legacy {
                            entry_id, boundary, ..
                        }) if *entry_id == unit.id => *boundary,
                        _ => TranscriptCommitBoundary::default(),
                    };
                    let wrapped = wrap_transcript_rows(width, &logical_rows, boundary);
                    entry_committable_prefix = wrapped.committable_prefix_rows;
                    rows.extend(wrapped.rows);
                    row_end_boundaries.extend(wrapped.row_end_boundaries.into_iter().map(Some));
                    previous_legacy_range = Some(ranges.len());
                }
                TranscriptPresentation::ResidentStream => {
                    let Some(PresentationOverride::Resident(tail)) = self.overrides.get(&unit.id)
                    else {
                        panic!("resident stream presentation must exist");
                    };
                    let block = compile_view(&tail.view, width);
                    let physically_complete = block.physically_complete;
                    rows.extend(block.rows);
                    row_end_boundaries.extend(std::iter::repeat_n(
                        None,
                        rows.len().saturating_sub(content_start),
                    ));
                    entry_committable_prefix = if physically_complete {
                        rows.len().saturating_sub(content_start)
                    } else {
                        0
                    };
                    previous_legacy_range = None;
                }
                TranscriptPresentation::View(view) => {
                    let mut physically_complete = true;
                    if let Some(PartialCommit::Semantic {
                        entry_id,
                        rows: frozen_rows,
                        committed_rows,
                    }) = partial_for_unit.flatten()
                    {
                        debug_assert_eq!(*entry_id, unit.id);
                        rows.extend(frozen_rows.iter().skip(*committed_rows).cloned());
                        row_end_boundaries.extend(std::iter::repeat_n(
                            None,
                            rows.len().saturating_sub(content_start),
                        ));
                    } else {
                        let mut block = compile_view(view, width);
                        physically_complete = block.physically_complete;
                        if unit.truncation == Truncation::Call {
                            block.rows =
                                truncate_view_rows(block.rows, DISPLAY_CALL_MAX_LINES + 1, true);
                        } else if unit.truncation == Truncation::Result {
                            block.rows = truncate_view_rows(block.rows, DISPLAY_MAX_LINES, false);
                        }
                        rows.extend(block.rows);
                        row_end_boundaries.extend(std::iter::repeat_n(
                            None,
                            rows.len().saturating_sub(content_start),
                        ));
                    }
                    entry_committable_prefix = if physically_complete {
                        rows.len().saturating_sub(content_start)
                    } else {
                        0
                    };
                    previous_legacy_range = None;
                }
            }

            ranges.push(RenderedEntryRange {
                unit_index,
                id: unit.id,
                lifecycle: unit.lifecycle,
                commit_mode: unit.commit_mode,
                semantic: matches!(
                    unit.presentation,
                    TranscriptPresentation::View(_) | TranscriptPresentation::ResidentStream
                ),
                rows: start..rows.len(),
                leading_flow_rows,
                committable_prefix_rows: entry_committable_prefix,
            });
        }

        trim_plain_trailing_physical_all(&mut rows, &mut row_end_boundaries, &mut ranges);
        let mut committable_rows = 0;
        for range in &ranges {
            match range.commit_mode {
                EntryCommitMode::Blocked => break,
                EntryCommitMode::Sealed | EntryCommitMode::SourceBackedAppendOnly => {
                    let content_start = range.rows.start + range.leading_flow_rows;
                    let physical_content_len =
                        range.rows.len().saturating_sub(range.leading_flow_rows);
                    let eligible = content_start + range.committable_prefix_rows;
                    committable_rows = eligible;
                    if range.committable_prefix_rows < physical_content_len {
                        // Impossible-fit row encountered in this unit; subsequent entries cannot leapfrog it.
                        break;
                    }
                }
            }
        }
        TranscriptRenderCache {
            width,
            rows,
            row_end_boundaries,
            ranges,
            committable_rows,
        }
    }

    pub(crate) fn ensure_render_cache(&mut self, width: u16) {
        let width = width.max(1);
        if self
            .rendered_rows_cache
            .as_ref()
            .is_some_and(|cache| cache.width == width)
        {
            return;
        }
        self.rendered_rows_cache = Some(self.mixed_render(width));
    }

    pub(crate) fn committable_len(&self) -> usize {
        self.rendered_rows_cache
            .as_ref()
            .map_or(0, |cache| cache.committable_rows)
    }

    #[cfg(test)]
    fn entry_lifecycles(&self) -> Vec<EntryLifecycle> {
        self.presentation_cache
            .iter()
            .map(|entry| entry.lifecycle)
            .collect()
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
        let Some(mut cache) = self.rendered_rows_cache.take() else {
            return;
        };

        let committed_rows = rows.min(cache.rows.len()).min(cache.committable_rows);
        if committed_rows == 0 {
            self.rendered_rows_cache = Some(cache);
            return;
        }

        let mut remaining = committed_rows;
        while remaining > 0 {
            let Some(range) = cache.ranges.first_mut() else {
                break;
            };
            let range_len = range.rows.len();
            let take = remaining.min(range_len);
            let consumed_leading = take.min(range.leading_flow_rows);
            if range.semantic {
                let partial = match self.commit_state.partial.take() {
                    Some(PartialCommit::Semantic {
                        entry_id,
                        rows,
                        committed_rows,
                    }) if entry_id == range.id => PartialCommit::Semantic {
                        entry_id,
                        rows,
                        committed_rows: committed_rows.saturating_add(take),
                    },
                    _ => PartialCommit::Semantic {
                        entry_id: range.id,
                        rows: cache.rows[range.rows.start..range.rows.end].to_vec(),
                        committed_rows: take,
                    },
                };
                self.commit_state.partial = Some(partial);
            } else {
                let existing_gap_committed = match self.commit_state.partial.as_ref() {
                    Some(PartialCommit::Legacy {
                        entry_id,
                        leading_gap_committed,
                        ..
                    }) if *entry_id == range.id => *leading_gap_committed,
                    _ => false,
                };
                let leading_gap_committed = existing_gap_committed || consumed_leading > 0;
                let boundary_index = range.rows.start.saturating_add(take.saturating_sub(1));
                let boundary = cache.row_end_boundaries[boundary_index].unwrap_or_default();
                self.commit_state.partial = Some(PartialCommit::Legacy {
                    entry_id: range.id,
                    boundary,
                    leading_gap_committed,
                });
            }
            if take == range_len {
                if range.commit_mode == EntryCommitMode::SourceBackedAppendOnly {
                    range.leading_flow_rows = 0;
                    remaining -= take;
                    continue;
                }
                if matches!(
                    self.overrides.get(&range.id),
                    Some(PresentationOverride::Resident(_))
                ) {
                    self.overrides.remove(&range.id);
                }
                self.commit_state.completed_prefix = range.unit_index.saturating_add(1);
                self.commit_state.partial = None;
                cache.ranges.remove(0);
            } else {
                range.leading_flow_rows = range.leading_flow_rows.saturating_sub(consumed_leading);
                remaining = 0;
                break;
            }
            remaining -= take;
        }
        cache.rows.drain(..committed_rows);
        cache.row_end_boundaries.drain(..committed_rows);
        cache.ranges.iter_mut().for_each(|range| {
            range.rows.start = range.rows.start.saturating_sub(committed_rows);
            range.rows.end = range.rows.end.saturating_sub(committed_rows);
        });
        cache.committable_rows = cache.committable_rows.saturating_sub(committed_rows);
        self.rendered_rows_cache = Some(cache);
    }
}

#[derive(Debug, Clone)]
struct TranscriptCommitState {
    completed_prefix: usize,
    partial: Option<PartialCommit>,
}

#[derive(Debug, Clone)]
enum PartialCommit {
    Legacy {
        entry_id: TranscriptId,
        boundary: TranscriptCommitBoundary,
        leading_gap_committed: bool,
    },
    Semantic {
        entry_id: TranscriptId,
        rows: Vec<Line<'static>>,
        committed_rows: usize,
    },
}

impl Default for TranscriptCommitState {
    fn default() -> Self {
        Self {
            completed_prefix: 0,
            partial: None,
        }
    }
}

impl PartialCommit {
    fn entry_id(&self) -> TranscriptId {
        match self {
            Self::Legacy { entry_id, .. } | Self::Semantic { entry_id, .. } => *entry_id,
        }
    }
}

impl TranscriptCommitState {}

fn entry_commit_mode(item: &TimelineItem, assistant_stream_open: bool) -> EntryCommitMode {
    match item {
        TimelineItem::AssistantMessage { .. } if assistant_stream_open => {
            EntryCommitMode::SourceBackedAppendOnly
        }
        TimelineItem::ToolCall {
            status:
                ToolTimelineStatus::PendingApproval
                | ToolTimelineStatus::Running
                | ToolTimelineStatus::Approved,
            ..
        } => EntryCommitMode::Blocked,
        _ => EntryCommitMode::Sealed,
    }
}

fn entry_lifecycle(item: &TimelineItem, assistant_stream_open: bool) -> EntryLifecycle {
    match item {
        TimelineItem::ToolCall { status, .. } => match status {
            ToolTimelineStatus::Finished
            | ToolTimelineStatus::Failed
            | ToolTimelineStatus::Rejected => EntryLifecycle::Sealed,
            _ => EntryLifecycle::Open,
        },
        TimelineItem::AssistantMessage { .. } if assistant_stream_open => EntryLifecycle::Open,
        _ => EntryLifecycle::Sealed,
    }
}

/// INTERNAL PRESENTATION MECHANICS.
///
/// Width-specific physical rendering cache retained below feature APIs.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptRenderCache {
    width: u16,
    rows: Vec<Line<'static>>,
    row_end_boundaries: Vec<Option<TranscriptCommitBoundary>>,
    ranges: Vec<RenderedEntryRange>,
    committable_rows: usize,
}

#[derive(Debug, Clone)]
struct RenderedEntryRange {
    unit_index: usize,
    id: TranscriptUnitId,
    lifecycle: EntryLifecycle,
    commit_mode: EntryCommitMode,
    semantic: bool,
    rows: Range<usize>,
    leading_flow_rows: usize,
    committable_prefix_rows: usize,
}

fn trim_plain_leading_legacy(rows: &mut Vec<TranscriptRow>) {
    while rows.first().is_some_and(is_blank_row) {
        rows.remove(0);
    }
}

fn trim_plain_trailing_physical(
    rows: &mut Vec<Line<'static>>,
    boundaries: &mut Vec<Option<TranscriptCommitBoundary>>,
    ranges: &mut [RenderedEntryRange],
    range_index: usize,
) {
    let Some(range) = ranges.get_mut(range_index) else {
        return;
    };
    // The assistant is the only remaining legacy family, and its rows never
    // carry a background, so a trailing empty row here is purely formatter
    // padding that flow owns and may drop.
    while !range.semantic
        && range.rows.end > range.rows.start
        && rows
            .get(range.rows.end - 1)
            .is_some_and(|row| row.spans.iter().all(|span| span.content.is_empty()))
    {
        rows.remove(range.rows.end - 1);
        boundaries.remove(range.rows.end - 1);
        range.rows.end -= 1;
    }

    let content_len = range.rows.len().saturating_sub(range.leading_flow_rows);
    range.committable_prefix_rows = range.committable_prefix_rows.min(content_len);
}

fn trim_plain_trailing_physical_all(
    rows: &mut Vec<Line<'static>>,
    boundaries: &mut Vec<Option<TranscriptCommitBoundary>>,
    ranges: &mut [RenderedEntryRange],
) {
    for index in (0..ranges.len()).rev() {
        trim_plain_trailing_physical(rows, boundaries, ranges, index);
        if ranges[index].rows.end > ranges[index].rows.start {
            break;
        }
    }
}

fn truncate_view_rows(
    mut rows: Vec<Line<'static>>,
    max_rows: usize,
    call: bool,
) -> Vec<Line<'static>> {
    if rows.len() <= max_rows {
        return rows;
    }
    let hidden = rows.len().saturating_sub(max_rows);
    rows.truncate(max_rows);
    let label = if call {
        format!("… {hidden} more argument lines")
    } else {
        let noun = if hidden == 1 { "line" } else { "lines" };
        format!("… {hidden} more {noun} (full result retained)")
    };
    rows.push(Line::styled(label, truncation_footer_style()));
    rows
}

fn is_blank_row(row: &TranscriptRow) -> bool {
    row.line
        .spans
        .iter()
        .all(|span| span.content.as_ref().is_empty())
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

#[test]
fn semantic_partial_commit_freezes_on_resize() {
    let mut transcript = TranscriptState::default();
    transcript.push_item(TimelineItem::ErrorMessage {
        text: "abcdefghijk".to_string(),
    });
    transcript.push_item(TimelineItem::AssistantMessage {
        segments: vec![AssistantSegment::Text("following content".to_string())],
    });

    transcript.ensure_render_cache(8);
    let original = transcript.uncommitted_rows().to_vec();
    let semantic_rows = original.len();
    assert!(semantic_rows > 2);
    let commit = 1;
    transcript.mark_rows_committed(commit);
    let frozen_tail = vec![original[1].clone()];

    transcript.ensure_render_cache(80);
    assert_eq!(
        &transcript.uncommitted_rows()[..frozen_tail.len()],
        frozen_tail
    );
}

#[test]
fn open_entry_blocks_commit_until_sealed() {
    let mut transcript = TranscriptState::default();
    transcript.upsert_tool_call(
        "open".to_string(),
        "read".to_string(),
        serde_json::Value::Null,
        ToolTimelineStatus::Running,
    );
    transcript.ensure_render_cache(80);
    assert_eq!(transcript.committable_len(), 0);
    transcript.finish_tool_call("open", false);
    transcript.ensure_render_cache(80);
    assert!(transcript.committable_len() > 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        presentation::stream::compile_stream,
        presentation::{HostedStream, StreamOffset},
        transcript::AssistantStream,
    };
    use ratatui::style::Modifier;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    /// Content text with trailing whitespace trimmed. A semantic full-width Box
    /// paints its background into trailing space spans; this lets tests compare
    /// content and background footprint without counting those decorative cells.
    fn trimmed_text(line: &Line<'static>) -> String {
        line_text(line).trim_end().to_string()
    }

    fn rendered_texts_trimmed(transcript: &TranscriptState) -> Vec<String> {
        transcript
            .uncommitted_rows()
            .iter()
            .map(trimmed_text)
            .collect()
    }

    fn text_segments(text: &str) -> Vec<AssistantSegment> {
        vec![AssistantSegment::Text(text.to_string())]
    }

    fn range_rows_for_unit(transcript: &TranscriptState, unit_id: TranscriptUnitId) -> usize {
        transcript
            .rendered_rows_cache
            .as_ref()
            .and_then(|cache| cache.ranges.iter().find(|range| range.id == unit_id))
            .map(|range| range.rows.len())
            .expect("unit must have a rendered range")
    }

    #[test]
    fn hosted_unit_waits_for_partially_committed_user_bubble() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "hello".to_string(),
        });
        let user_id = transcript.entry_ids[0];
        let hosted = transcript.begin_hosted_assistant_unit();
        transcript.seal_hosted_assistant_unit(
            hosted.id,
            vec![AssistantSegment::Text("assistant answer".to_string())],
        );
        transcript.ensure_render_cache(80);

        // The semantic bubble is top padding, content, bottom padding. Freeze only
        // its first two physical rows, leaving the bottom background row partial.
        assert_eq!(range_rows_for_unit(&transcript, user_id), 3);
        transcript.mark_rows_committed(2);
        assert!(matches!(
            transcript.commit_state.partial,
            Some(PartialCommit::Semantic {
                entry_id,
                committed_rows: 2,
                ..
            }) if entry_id == user_id
        ));
        assert!(!transcript.hosted_unit_is_history_head(hosted.id));

        // The next ordered history payload is exactly U2. The hosted unit has no
        // permission to contribute rows while that predecessor remains partial.
        transcript.ensure_render_cache(80);
        assert_eq!(transcript.committable_len(), 1);
        assert_eq!(transcript.uncommitted_rows().len(), 1);
        assert!(
            transcript.uncommitted_rows()[0].style.bg.is_some()
                || transcript.uncommitted_rows()[0]
                    .spans
                    .iter()
                    .any(|span| span.style.bg.is_some())
        );
        assert!(
            !transcript
                .uncommitted_rows()
                .iter()
                .any(|row| line_text(row).contains("assistant answer"))
        );

        transcript.mark_rows_committed(1);
        assert!(transcript.commit_state.partial.is_none());
        assert!(transcript.hosted_unit_is_history_head(hosted.id));
    }

    #[test]
    fn completed_prefix_uses_presentation_unit_indices() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "first".to_string(),
        });
        transcript.push_item(TimelineItem::UserMessage {
            text: "second".to_string(),
        });
        let registration = transcript.begin_hosted_assistant_unit();
        transcript.seal_hosted_assistant_unit(
            registration.id,
            vec![AssistantSegment::Text("answer".to_string())],
        );
        transcript.ensure_render_cache(80);

        let user_rows = transcript.committable_len();
        assert!(user_rows > 0);
        transcript.mark_rows_committed(user_rows);
        assert_eq!(transcript.commit_state.completed_prefix, 1);
        assert!(transcript.hosted_unit_is_history_head(registration.id));

        transcript.finish_stream_unit(registration.id);
        assert_eq!(transcript.commit_state.completed_prefix, 2);
        assert!(matches!(
            transcript.presentation_cache[1].ownership,
            PresentationOwnership::NativeHistory
        ));
    }

    #[test]
    fn resident_tail_starts_at_the_committed_stream_cursor() {
        let mut transcript = TranscriptState::default();
        let registration = transcript.begin_hosted_assistant_unit();
        let mut stream = HostedStream::new(
            registration.id,
            AssistantStream::new(),
            registration.leading_boundary,
        );
        stream
            .content_mut()
            .push_delta(SegmentKind::Text, "a\nb\nc");
        transcript
            .seal_hosted_assistant_unit(registration.id, stream.content().segments().to_vec());

        let first = stream.prepare_frame(80, 1);
        assert_eq!(first.history.rows.len(), 1);
        stream.apply_commit_success(first.history);
        let committed_snapshot = {
            stream.seal();
            stream.snapshot()
        };
        assert!(stream.can_handoff());
        assert!(committed_snapshot.source_base > StreamOffset::ZERO);

        let handoff = stream.into_resident_handoff();
        let expected_rows =
            compile_stream(&committed_snapshot.view, 80, committed_snapshot.source_end).rows;
        let source_base = handoff.source_base;
        let source_end = handoff.source_end;
        transcript.adopt_resident_stream(handoff);
        transcript.ensure_render_cache(80);

        assert_eq!(source_base, StreamOffset::new(2));
        assert_eq!(source_end, StreamOffset::new(5));
        assert_eq!(transcript.uncommitted_rows(), expected_rows.as_slice());
        assert!(
            !transcript
                .uncommitted_rows()
                .iter()
                .any(|row| line_text(row).contains('a'))
        );

        let resident_rows = transcript.uncommitted_len();
        transcript.mark_rows_committed(resident_rows);
        assert!(!transcript.overrides.contains_key(&registration.id));
        assert_eq!(transcript.commit_state.completed_prefix, 1);
    }

    #[test]
    fn resident_impossible_width_remains_uncommittable_until_resize() {
        let mut transcript = TranscriptState::default();
        let registration = transcript.begin_hosted_assistant_unit();
        let mut stream = HostedStream::new(
            registration.id,
            AssistantStream::new(),
            registration.leading_boundary,
        );
        stream.content_mut().push_delta(SegmentKind::Text, "漢");
        stream.seal();
        transcript
            .seal_hosted_assistant_unit(registration.id, stream.content().segments().to_vec());
        transcript.adopt_resident_stream(stream.into_resident_handoff());

        transcript.ensure_render_cache(1);
        assert!(!transcript.uncommitted_rows().is_empty());
        assert_eq!(transcript.committable_len(), 0);

        transcript.ensure_render_cache(2);
        assert!(transcript.committable_len() > 0);
    }

    #[test]
    fn hosted_units_advance_through_one_global_history_frontier() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "user 1".to_string(),
        });
        let user1 = transcript.entry_ids[0];
        let assistant1 = transcript.begin_hosted_assistant_unit();
        let mut stream1 = HostedStream::new(
            assistant1.id,
            AssistantStream::new(),
            assistant1.leading_boundary,
        );
        stream1
            .content_mut()
            .push_delta(SegmentKind::Text, "assistant 1 long response");
        stream1.seal();
        transcript.seal_hosted_assistant_unit(assistant1.id, stream1.content().segments().to_vec());

        transcript.push_item(TimelineItem::UserMessage {
            text: "user 2".to_string(),
        });
        let user2 = transcript.entry_ids[2];
        let assistant2 = transcript.begin_hosted_assistant_unit();
        let mut stream2 = HostedStream::new(
            assistant2.id,
            AssistantStream::new(),
            assistant2.leading_boundary,
        );
        stream2
            .content_mut()
            .push_delta(SegmentKind::Text, "assistant 2 long response");
        stream2.seal();
        transcript.seal_hosted_assistant_unit(assistant2.id, stream2.content().segments().to_vec());

        let mut owners = Vec::new();
        transcript.ensure_render_cache(80);
        assert!(!transcript.hosted_unit_is_history_head(assistant1.id));

        owners.push(user1);
        transcript.mark_rows_committed(range_rows_for_unit(&transcript, user1));
        assert!(transcript.hosted_unit_is_history_head(assistant1.id));

        owners.push(assistant1.id);
        assert!(stream1.can_handoff());
        transcript.adopt_resident_stream(stream1.into_resident_handoff());
        transcript.ensure_render_cache(80);
        transcript.mark_rows_committed(range_rows_for_unit(&transcript, assistant1.id));
        assert!(!transcript.hosted_unit_is_history_head(assistant2.id));

        transcript.ensure_render_cache(80);
        owners.push(user2);
        transcript.mark_rows_committed(range_rows_for_unit(&transcript, user2));
        assert!(transcript.hosted_unit_is_history_head(assistant2.id));

        owners.push(assistant2.id);
        assert!(stream2.can_handoff());
        transcript.adopt_resident_stream(stream2.into_resident_handoff());
        transcript.ensure_render_cache(80);
        transcript.mark_rows_committed(range_rows_for_unit(&transcript, assistant2.id));

        assert_eq!(owners, vec![user1, assistant1.id, user2, assistant2.id]);
        assert_eq!(transcript.commit_state.completed_prefix, 4);
        assert!(transcript.commit_state.partial.is_none());
    }

    fn rendered_texts(transcript: &TranscriptState) -> Vec<String> {
        transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect()
    }

    #[test]
    fn user_message_formats_to_a_semantic_bubble() {
        let formatter = TuiFormatter::default();
        let item = TimelineItem::UserMessage {
            text: "line1\nline2\n".to_string(),
        };

        assert!(
            matches!(formatter.format(&item), TranscriptPresentation::View(_)),
            "user messages are semantic views"
        );
    }

    /// Display column of the first non-whitespace grapheme in a physical row.
    /// This measures the actual horizontal inset, ignoring nothing: leading spaces
    /// in the emitted spans count as columns the text was pushed right by.
    fn first_content_column(line: &Line<'static>) -> usize {
        use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        text.chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| c.width().unwrap_or(0))
            .sum()
    }

    /// Reconstructs the legacy user bubble exactly as the old formatter and wrap
    /// step produced it (`TranscriptRow::new`, which is `RowLayout::zero()`, then
    /// `wrap_transcript_rows`), and returns its content row so callers can measure
    /// the oracle geometry independent of the new semantic path.
    fn legacy_user_content_row(text: &str, width: u16) -> Line<'static> {
        let style = Line::from("").style.bg(ratatui::style::Color::White);
        let owned = text.to_string();
        let rows = [
            TranscriptRow::new(Line::styled("", style)),
            TranscriptRow::new(Line::styled(owned, style)),
            TranscriptRow::new(Line::styled("", style)),
        ];
        let wrapped = wrap_transcript_rows(width, &rows, TranscriptCommitBoundary::default());
        wrapped.rows[1].clone()
    }

    #[test]
    fn user_bubble_horizontal_geometry_matches_legacy() {
        const WIDTH: u16 = 12;

        // Oracle: old legacy pixel geometry for a short message.
        let legacy = legacy_user_content_row("hello", WIDTH);
        let legacy_x = first_content_column(&legacy);

        let mut t = TranscriptState::default();
        t.push_item(TimelineItem::UserMessage {
            text: "hello".to_string(),
        });
        t.ensure_render_cache(WIDTH);
        let rows = t.uncommitted_rows();
        let content = &rows[1];
        let semantic_x = first_content_column(content);

        // Text starts at the same column, and (verified against the oracle) that
        // column is 0 — the user bubble had NO horizontal inset, unlike the
        // assistant/tool OUTER_MARGIN of 2.
        assert_eq!(legacy_x, 0, "legacy user text should sit at column 0");
        assert_eq!(
            semantic_x, 0,
            "semantic user bubble must match legacy at x=0"
        );
        assert_eq!(semantic_x, legacy_x);

        // Background covers the full width in both (col 0 and the last col).
        let bg = |line: &Line<'static>| {
            line.style.bg.is_some() || line.spans.iter().any(|sp| sp.style.bg.is_some())
        };
        assert!(bg(&legacy));
        assert!(bg(content));

        // Usable wrap width: a 12-wide message fits one row only if the usable
        // width is 12 (not 8). Verify legacy and semantic behave identically.
        let fit_legacy = legacy_user_content_row("0123456789AB", WIDTH);
        let fit_semantic_rows = {
            let mut t2 = TranscriptState::default();
            t2.push_item(TimelineItem::UserMessage {
                text: "0123456789AB".to_string(),
            });
            t2.ensure_render_cache(WIDTH);
            t2.uncommitted_rows()
                .iter()
                .filter(|r| {
                    let txt: String = r.spans.iter().map(|s| s.content.as_ref()).collect();
                    !txt.trim().is_empty()
                })
                .count()
        };
        assert_eq!(
            first_content_column(&fit_legacy),
            0,
            "a 12-wide message must not be pushed right"
        );
        let legacy_fit_rows = {
            let style = Line::from("").style.bg(ratatui::style::Color::White);
            let rows = [TranscriptRow::new(Line::styled("0123456789AB", style))];
            wrap_transcript_rows(WIDTH, &rows, TranscriptCommitBoundary::default())
                .rows
                .iter()
                .filter(|r| {
                    let txt: String = r.spans.iter().map(|s| s.content.as_ref()).collect();
                    !txt.trim().is_empty()
                })
                .count()
        };
        assert_eq!(
            legacy_fit_rows, 1,
            "legacy usable width should hold 12 chars"
        );
        assert_eq!(fit_semantic_rows, legacy_fit_rows);
    }

    #[test]
    fn error_spacing_is_owned_by_flow_except_first_entry_padding() {
        let mut user_error = TranscriptState::default();
        user_error.push_item(TimelineItem::UserMessage {
            text: "user".to_string(),
        });
        user_error.push_item(TimelineItem::ErrorMessage {
            text: "error".to_string(),
        });
        user_error.ensure_render_cache(80);
        assert_eq!(
            rendered_texts_trimmed(&user_error),
            vec!["", "user", "", "", "error"]
        );

        let mut assistant_error = TranscriptState::default();
        assistant_error.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("answer"),
        });
        assistant_error.push_item(TimelineItem::ErrorMessage {
            text: "error".to_string(),
        });
        assistant_error.ensure_render_cache(80);
        assert_eq!(
            rendered_texts_trimmed(&assistant_error),
            vec!["", "  answer", "", "error"]
        );

        let mut error_user = TranscriptState::default();
        error_user.push_item(TimelineItem::ErrorMessage {
            text: "error".to_string(),
        });
        error_user.push_item(TimelineItem::UserMessage {
            text: "user".to_string(),
        });
        error_user.ensure_render_cache(80);
        assert_eq!(
            rendered_texts_trimmed(&error_user),
            vec!["", "error", "", "", "user", ""]
        );

        let mut first_error = TranscriptState::default();
        first_error.push_item(TimelineItem::ErrorMessage {
            text: "error".to_string(),
        });
        first_error.ensure_render_cache(80);
        assert_eq!(rendered_texts_trimmed(&first_error), vec!["", "error"]);
    }

    #[test]
    fn wraps_transcript_rows_to_visual_width() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("abcdef"),
        });

        // width 6 minus outer margins (2+2) = content width 2.
        transcript.ensure_render_cache(6);

        assert_eq!(
            rendered_texts_trimmed(&transcript),
            ["", "  ab", "  cd", "  ef"]
        );
    }

    #[test]
    fn impossible_fit_grapheme_does_not_commit_and_commits_once_after_resize() {
        let mut transcript = TranscriptState::default();
        transcript
            .append_assistant_stream_fragment(vec![AssistantSegment::Text("漢字".to_string())]);

        // At width 1, the 2-cell CJK graphemes cannot physically fit in the content track.
        // Only the leading blank flow gap is committable; zero oversized text rows are committable.
        transcript.ensure_render_cache(1);
        let initial_committable = transcript.committable_len();
        assert_eq!(
            initial_committable, 1,
            "only the leading flow gap is committable; zero oversized text rows are committable at width 1"
        );

        // Committing the leading gap leaves 0 committable rows; commit cursor does not advance into the text.
        transcript.mark_rows_committed(initial_committable);
        transcript.ensure_render_cache(1);
        assert_eq!(
            transcript.committable_len(),
            0,
            "no further rows are committable while width is too small"
        );

        // Now resize to width 20 where "漢字" fits normally.
        transcript.ensure_render_cache(20);
        assert_eq!(
            transcript.committable_len(),
            1,
            "assistant row becomes committable once width accommodates it"
        );

        // Commit the row.
        transcript.mark_rows_committed(1);

        // Re-render: the committed content has entered history and does not duplicate in uncommitted rows.
        transcript.ensure_render_cache(20);
        let uncommitted = rendered_texts(&transcript).join("|");
        assert!(
            !uncommitted.contains("漢字"),
            "committed content must not duplicate in uncommitted suffix"
        );
        assert_eq!(transcript.committable_len(), 0);
    }

    #[test]
    fn impossible_fit_grapheme_remains_uncommittable_after_assistant_stream_finish() {
        let mut transcript = TranscriptState::default();
        transcript
            .append_assistant_stream_fragment(vec![AssistantSegment::Text("漢字".to_string())]);

        // At width 1, only the leading flow gap is committable.
        transcript.ensure_render_cache(1);
        let initial_committable = transcript.committable_len();
        assert_eq!(
            initial_committable, 1,
            "only the leading flow gap is committable at width 1"
        );
        transcript.mark_rows_committed(initial_committable);
        transcript.ensure_render_cache(1);
        assert_eq!(transcript.committable_len(), 0);

        // Seal the assistant stream while STILL at width 1.
        transcript.finish_assistant_stream();
        transcript.ensure_render_cache(1);

        // The oversized assistant rows must STILL not be committable despite lifecycle being Sealed.
        assert_eq!(
            transcript.committable_len(),
            0,
            "oversized rows remain uncommittable after stream sealing at width 1"
        );

        // Now resize to width 20 where "漢字" fits normally.
        transcript.ensure_render_cache(20);
        let committable = transcript.committable_len();
        assert!(
            committable > 0,
            "assistant becomes committable once width accommodates it"
        );

        // Commit.
        transcript.mark_rows_committed(committable);
        transcript.ensure_render_cache(20);
        let uncommitted = rendered_texts(&transcript).join("|");
        assert!(
            !uncommitted.contains("漢字"),
            "漢字 must appear exactly once in native history and not duplicate"
        );
        assert_eq!(transcript.committable_len(), 0);
    }

    #[test]
    fn open_assistant_spilled_prefix_remains_committable_and_appends_after_resize() {
        let mut transcript = TranscriptState::default();
        transcript
            .append_assistant_stream_fragment(vec![AssistantSegment::Text("A\nB\nC".to_string())]);
        transcript.ensure_render_cache(20);
        let before = rendered_texts(&transcript);
        assert!(before.iter().any(|text| text.contains('A')));
        assert!(transcript.committable_len() > 0);

        let committed = transcript.committable_len();
        transcript.mark_rows_committed(committed);
        transcript.ensure_render_cache(8);

        transcript
            .append_assistant_stream_fragment(vec![AssistantSegment::Text("\nD\nE".to_string())]);
        transcript.ensure_render_cache(20);
        let after = rendered_texts(&transcript).join("|");
        assert!(!after.contains('A'));
        assert!(!after.contains('B'));
        assert!(!after.contains('C'));
        assert!(after.contains('D'));
        assert!(after.contains('E'));
    }

    #[test]
    fn open_assistant_zero_committed_prefix_finalizes_to_view_without_jump() {
        let mut transcript = TranscriptState::default();
        transcript.append_assistant_stream_fragment(vec![AssistantSegment::Text(
            "Hello world".to_string(),
        )]);
        transcript.ensure_render_cache(80);

        // Before finalization: open stream on SourceBackedAssistantRows.
        assert!(matches!(
            transcript.presentation_cache[0].presentation,
            TranscriptPresentation::SourceBackedAssistantRows(_)
        ));

        // Finish the stream without committing anything.
        transcript.finish_assistant_stream();
        transcript.ensure_render_cache(80);

        // After finalization: switches cleanly to View.
        assert!(matches!(
            transcript.presentation_cache[0].presentation,
            TranscriptPresentation::View(_)
        ));

        let uncommitted = rendered_texts_trimmed(&transcript);
        assert_eq!(uncommitted, vec!["", "  Hello world"]);
    }

    #[test]
    fn open_assistant_with_committed_prefix_stays_pinned_to_source_backed_and_emits_once() {
        let mut transcript = TranscriptState::default();
        transcript.append_assistant_stream_fragment(vec![AssistantSegment::Text(
            "Row 1\nRow 2\nRow 3".to_string(),
        )]);
        transcript.ensure_render_cache(80);

        // Commit part of the stream (e.g. leading flow gap + Row 1).
        transcript.mark_rows_committed(2);
        transcript.ensure_render_cache(80);

        // Seal the stream.
        transcript.finish_assistant_stream();
        transcript.ensure_render_cache(80);

        // Unit stays pinned to SourceBackedAssistantRows.
        assert!(matches!(
            transcript.presentation_cache[0].presentation,
            TranscriptPresentation::SourceBackedAssistantRows(_)
        ));
        assert!(matches!(
            transcript.commit_state.partial,
            Some(PartialCommit::Legacy { .. })
        ));

        let uncommitted = rendered_texts_trimmed(&transcript);
        assert!(!uncommitted.iter().any(|s| s.contains("Row 1")));
        assert!(uncommitted.iter().any(|s| s.contains("Row 2")));
        assert!(uncommitted.iter().any(|s| s.contains("Row 3")));

        // Commit remaining rows.
        let remaining_committable = transcript.committable_len();
        transcript.mark_rows_committed(remaining_committable);
        transcript.ensure_render_cache(80);

        assert_eq!(transcript.commit_state.completed_prefix, 1);
        assert!(transcript.commit_state.partial.is_none());
    }

    #[test]
    fn finalized_pinned_legacy_assistant_followed_by_user_message_has_one_flow_gap() {
        let mut transcript = TranscriptState::default();
        transcript.append_assistant_stream_fragment(vec![AssistantSegment::Text(
            "Long assistant line 1\nLong assistant line 2".to_string(),
        )]);
        transcript.ensure_render_cache(80);

        // Commit prefix.
        transcript.mark_rows_committed(2);
        transcript.ensure_render_cache(80);

        // Finalize stream and append a User message.
        transcript.finish_assistant_stream();
        transcript.push_item(TimelineItem::UserMessage {
            text: "Next prompt".to_string(),
        });
        transcript.ensure_render_cache(80);

        let texts = rendered_texts_trimmed(&transcript);
        // Ensure there is exactly one flow gap before the user message:
        // [assistant last row, single flow gap, user bubble top padding, user message, user bubble bottom padding]
        assert_eq!(
            texts,
            vec!["  Long assistant line 2", "", "", "Next prompt", ""]
        );
    }

    #[test]
    fn commit_boundary_survives_resize_after_partial_line_commit() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("abcdef"),
        });

        transcript.ensure_render_cache(6);
        transcript.mark_rows_committed(2);
        // After committing the first two physical rows (blank + "  ab"), the
        // boundary lands after "ab"; the remaining content re-wraps at width 6
        // (content width 2) as "cd" / "ef".
        assert_eq!(rendered_texts_trimmed(&transcript), ["  cd", "  ef"]);
    }

    #[test]
    fn partial_legacy_commit_can_consume_only_owned_flow_gap() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "user".to_string(),
        });
        transcript.push_item(TimelineItem::ErrorMessage {
            text: "legacy follows".to_string(),
        });
        transcript.ensure_render_cache(80);

        let user_rows = transcript.rendered_rows_cache.as_ref().unwrap().ranges[0]
            .rows
            .len();
        transcript.mark_rows_committed(user_rows + 1);
        transcript.ensure_render_cache(20);

        assert_eq!(rendered_texts(&transcript), vec!["legacy follows"]);
    }

    #[test]
    fn partial_semantic_commit_freezes_rows_and_reflows_following_content() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::ToolResult {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            text: "one two three four five six".to_string(),
            details: serde_json::Value::Null,
            is_error: false,
            collapsed: true,
        });
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("following content that reflows"),
        });
        transcript.ensure_render_cache(12);

        let cache = transcript.rendered_rows_cache.as_ref().unwrap();
        let semantic_end = cache.ranges[0].rows.end;
        let original = transcript.uncommitted_rows().to_vec();
        let committed = 2;
        let frozen_tail = original[committed..semantic_end].to_vec();
        transcript.mark_rows_committed(committed);

        transcript.ensure_render_cache(80);
        assert_eq!(
            &transcript.uncommitted_rows()[..frozen_tail.len()],
            frozen_tail
        );
        let text = rendered_texts(&transcript).join("|");
        assert!(text.contains("following content that reflows"));
    }

    #[test]
    fn fully_committed_semantic_unit_does_not_reappear_after_resize() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::ToolResult {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            text: "semantic result body".to_string(),
            details: serde_json::Value::Null,
            is_error: false,
            collapsed: true,
        });
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("following"),
        });
        transcript.ensure_render_cache(20);
        let semantic_end = transcript.rendered_rows_cache.as_ref().unwrap().ranges[0]
            .rows
            .end;
        transcript.mark_rows_committed(semantic_end);
        transcript.ensure_render_cache(80);
        let text = rendered_texts(&transcript).join("|");
        assert!(!text.contains("semantic result body"));
        assert!(text.contains("following"));
    }

    #[test]
    fn open_entry_blocks_commit_then_sealing_allows_it() {
        let mut transcript = TranscriptState::default();
        transcript.upsert_tool_call(
            "open".to_string(),
            "read".to_string(),
            serde_json::Value::Null,
            ToolTimelineStatus::Running,
        );
        transcript.ensure_render_cache(80);
        assert_eq!(transcript.committable_len(), 0);

        transcript.finish_tool_call("open", false);
        transcript.ensure_render_cache(80);
        let count = transcript.committable_len();
        assert!(count > 0);
        transcript.mark_rows_committed(count);
        assert_eq!(transcript.uncommitted_len(), 0);
    }

    #[test]
    fn mixed_legacy_semantic_partial_commit_survives_resize() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "legacy".to_string(),
        });
        transcript.push_item(TimelineItem::ErrorMessage {
            text: "semantic error with enough content to wrap".to_string(),
        });
        transcript.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("following mixed content"),
        });
        transcript.ensure_render_cache(16);

        let cache = transcript.rendered_rows_cache.as_ref().unwrap();
        let semantic_range = cache.ranges[1].rows.clone();
        let original = transcript.uncommitted_rows().to_vec();
        let committed = semantic_range.start + 1;
        let frozen_tail = original[committed..semantic_range.end].to_vec();
        transcript.mark_rows_committed(committed);

        transcript.ensure_render_cache(80);
        assert_eq!(
            &transcript.uncommitted_rows()[..frozen_tail.len()],
            frozen_tail
        );
        assert!(
            rendered_texts(&transcript)
                .join("|")
                .contains("following mixed content")
        );
    }

    #[test]
    #[should_panic(expected = "fully committed transcript unit")]
    fn committed_entry_mutation_panics_in_debug() {
        let mut transcript = TranscriptState::default();
        transcript.upsert_tool_call(
            "1".to_string(),
            "read".to_string(),
            serde_json::Value::Null,
            ToolTimelineStatus::Running,
        );
        transcript.finish_tool_call("1", false);
        transcript.ensure_render_cache(80);
        let count = transcript.committable_len();
        transcript.mark_rows_committed(count);
        transcript.update_tool_call_status(
            "1".to_string(),
            "read".to_string(),
            ToolTimelineStatus::Finished,
        );
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
        // kept as its own colored row. The lower composer gap is layout-owned:
        // [leading blank, a, b, c, bubble-bottom-blank].
        // One structural semantic bubble owned by a Box: one top padding row of
        // full-width background, then `a`/`b`/`c` with no blank between (column
        // gap 0), one bottom padding row. No state blank sits between messages.
        transcript.ensure_render_cache(80);
        let texts = rendered_texts_trimmed(&transcript);
        assert_eq!(texts, vec!["", "a", "b", "c", ""]);
        let bg_rows: Vec<bool> = transcript
            .uncommitted_rows()
            .iter()
            .map(|r| r.style.bg.is_some() || r.spans.iter().any(|s| s.style.bg.is_some()))
            .collect();
        // Background fills every transcript-owned row in the bubble.
        assert_eq!(bg_rows, vec![true, true, true, true, true]);
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

        transcript.ensure_render_cache(80);
        let texts = rendered_texts_trimmed(&transcript);
        // [\n, a, \n, \n, hi, \n, \n, b, \n] — a and b are separated by the
        // assistant message. Each item keeps its own bubble padding (backgrounded
        // blank) and a plain separator is added between items.
        assert_eq!(texts, vec!["", "a", "", "", "  hi", "", "", "b", ""]);
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
        // DISPLAY_MAX_LINES + a footer hint. The lower composer gap is layout-owned.
        transcript.ensure_render_cache(80);
        let rows = transcript.uncommitted_rows();
        assert_eq!(rows.len(), 15 + 1, "15 kept + footer");

        let footer_text: String = rows[15].spans.iter().map(|s| s.content.as_ref()).collect();
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

        // 1 title + 2 body = 3 rows (under the cap).
        transcript.ensure_render_cache(80);
        let rows = transcript.uncommitted_rows();
        assert_eq!(rows.len(), 3);
        let texts: Vec<String> = rows
            .iter()
            .map(|row| line_text(row).trim_start().to_string())
            .collect();
        assert_eq!(texts, vec!["read result", "one", "two"]);
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
        // (the assembler owns separators).
        transcript.ensure_render_cache(80);
        let rows = transcript.uncommitted_rows();
        assert_eq!(rows.len(), 1);
        let texts: Vec<String> = rows
            .iter()
            .map(|row| line_text(row).trim_start_matches("● ").to_string())
            .collect();
        assert_eq!(texts, vec!["tool * — finished"]);
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

        transcript.ensure_render_cache(80);
        let rows = transcript.uncommitted_rows();
        // structure: [colored-blank, hello, colored-blank(bubble bottom), plain
        // separator, t] — the bubble's padded bottom row stays part of the bubble,
        // and a fresh plain gap separates it from the agent message.
        let texts = rendered_texts_trimmed(&transcript);
        assert_eq!(texts, vec!["", "hello", "", "", "  t"]);

        // The bubble's bottom padding (rows[2]) must keep the background; the gap
        // (rows[3]) is a plain separator.
        assert!(
            rows[2].style.bg.is_some() || rows[2].spans.iter().any(|span| span.style.bg.is_some())
        );
        assert!(
            rows[3].style.bg.is_none() && !rows[3].spans.iter().any(|span| span.style.bg.is_some())
        );
    }

    #[test]
    fn user_bubble_keeps_bottom_background_as_last_item() {
        // A just-sent user message is the last item. Its colored bottom padding must
        // stay part of the bubble; the lower composer gap is layout-owned.
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::UserMessage {
            text: "hello".to_string(),
        });

        transcript.ensure_render_cache(80);
        let rows = transcript.uncommitted_rows();
        let texts = rendered_texts_trimmed(&transcript);
        // [colored-blank, hello, colored-blank(bubble bottom)]
        assert_eq!(texts, vec!["", "hello", ""]);
        assert!(
            rows[2].style.bg.is_some() || rows[2].spans.iter().any(|span| span.style.bg.is_some())
        );
        assert_eq!(rows.len(), 3);
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

        transcript.ensure_render_cache(80);
        let texts: Vec<String> = transcript
            .uncommitted_rows()
            .iter()
            .map(|row| {
                line_text(row)
                    .trim_start_matches("● ")
                    .trim_start()
                    .to_string()
            })
            .collect();
        // [call1, result-title, one, two, SEPARATOR, call2] — the call and its own
        // result touch with no blank, and a single blank precedes the next call.
        assert_eq!(
            texts,
            vec![
                "read ... — finished", // call1
                "read result",
                "one",
                "two",
                "",                    // separator
                "read ... — finished", // call2
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

    fn has_bg(line: &Line<'static>) -> bool {
        line.style.bg.is_some() || line.spans.iter().any(|s| s.style.bg.is_some())
    }

    fn user(messages: &[&str]) -> Vec<TimelineItem> {
        messages
            .iter()
            .map(|text| TimelineItem::UserMessage {
                text: text.to_string(),
            })
            .collect()
    }

    fn push_all(t: &mut TranscriptState, items: Vec<TimelineItem>) {
        for item in items {
            t.push_item(item);
        }
    }

    #[test]
    fn single_user_bubble_is_one_semantic_unit() {
        let mut t = TranscriptState::default();
        push_all(&mut t, user(&["hello"]));
        t.ensure_render_cache(80);
        assert_eq!(rendered_texts_trimmed(&t), vec!["", "hello", ""]);
        // Full-width background on every bubble row. Content stays at column 0
        // (no horizontal inset).
        let bg: Vec<bool> = t.uncommitted_rows().iter().map(has_bg).collect();
        assert_eq!(bg, vec![true, true, true]);
        assert_eq!(
            t.uncommitted_rows()[1]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
                .trim_end(),
            "hello"
        );
    }

    #[test]
    fn user_batch_then_assistant_has_one_flow_separator() {
        let mut t = TranscriptState::default();
        push_all(&mut t, user(&["a", "b"]));
        t.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("hi"),
        });
        t.ensure_render_cache(80);
        // bubble(top, a, b, bottom) + one plain flow gap + assistant(indent hi).
        assert_eq!(
            rendered_texts_trimmed(&t),
            vec!["", "a", "b", "", "", "  hi"]
        );
    }

    #[test]
    fn assistant_then_user_batch_has_one_flow_separator() {
        let mut t = TranscriptState::default();
        t.push_item(TimelineItem::AssistantMessage {
            segments: text_segments("hi"),
        });
        push_all(&mut t, user(&["a", "b"]));
        t.ensure_render_cache(80);
        assert_eq!(
            rendered_texts_trimmed(&t),
            vec!["", "  hi", "", "", "a", "b", ""]
        );
    }

    #[test]
    fn user_batch_and_tool_have_one_flow_gap_each_way() {
        // user batch -> finished tool call.
        let mut t = TranscriptState::default();
        push_all(&mut t, user(&["a", "b"]));
        t.push_item(TimelineItem::ToolCall {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::Value::Null,
            status: ToolTimelineStatus::Finished,
        });
        t.ensure_render_cache(80);
        let texts = rendered_texts_trimmed(&t);
        // bubble(top,a,b,bottom) + one gap + tool header.
        assert_eq!(texts, vec!["", "a", "b", "", "", "● read ... — finished"]);

        // tool call -> user batch.
        let mut t2 = TranscriptState::default();
        t2.push_item(TimelineItem::ToolCall {
            tool_call_id: "1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::Value::Null,
            status: ToolTimelineStatus::Finished,
        });
        push_all(&mut t2, user(&["a", "b"]));
        t2.ensure_render_cache(80);
        let texts2 = rendered_texts_trimmed(&t2);
        assert_eq!(texts2, vec!["● read ... — finished", "", "", "a", "b", ""]);
    }

    #[test]
    fn user_batch_and_error_have_one_flow_gap_each_way() {
        let mut t = TranscriptState::default();
        push_all(&mut t, user(&["a", "b"]));
        t.push_item(TimelineItem::ErrorMessage {
            text: "oops".to_string(),
        });
        t.ensure_render_cache(80);
        assert_eq!(
            rendered_texts_trimmed(&t),
            vec!["", "a", "b", "", "", "oops"]
        );

        let mut t2 = TranscriptState::default();
        t2.push_item(TimelineItem::ErrorMessage {
            text: "oops".to_string(),
        });
        push_all(&mut t2, user(&["a", "b"]));
        t2.ensure_render_cache(80);
        assert_eq!(
            rendered_texts_trimmed(&t2),
            vec!["", "oops", "", "", "a", "b", ""]
        );
    }

    #[test]
    fn user_bubble_keeps_background_and_padding_at_all_widths() {
        let message = "a fairly long user message that wraps at narrow widths";
        for width in [80u16, 20, 8, 3, 1] {
            let mut t = TranscriptState::default();
            push_all(&mut t, user(&["a", message]));
            t.ensure_render_cache(width);
            let rows = t.uncommitted_rows();
            assert!(rows.len() >= 3, "width {width}: got {}", rows.len());
            // Top and bottom padding rows always carry the full background.
            assert!(has_bg(&rows[0]), "width {width} top padding lost bg");
            assert!(has_bg(rows.last().unwrap()), "width {width} bottom lost bg");
            // a and message occupy distinct rows (gap 0 inside the bubble). Wrapping
            // may split the long message across rows, so assert the concatenated
            // content preserves it rather than any single row.
            let texts: Vec<String> = rows.iter().map(line_text).collect();
            assert!(texts.iter().any(|t| t.trim_end() == "a"));
            let joined: String = texts.concat();
            assert!(
                joined.replace(' ', "").contains("afairlylong"),
                "width {width}: message lost during wrap: {joined:?}"
            );
        }
    }

    #[test]
    fn user_batch_partial_commit_freezes_tail_on_resize() {
        let mut t = TranscriptState::default();
        push_all(&mut t, user(&["line0", "line1", "line2", "line3"]));
        t.ensure_render_cache(20);
        let original = t.uncommitted_rows().to_vec();

        // Partially commit the first two rows (top padding + line0).
        let committed = 2;
        t.mark_rows_committed(committed);
        let frozen_tail = &original[committed..];

        t.ensure_render_cache(80);
        let now = t.uncommitted_rows();
        // Committed rows do not return; the remaining bubble rows stay frozen.
        assert!(now.len() >= frozen_tail.len());
        assert_eq!(
            &now[..frozen_tail.len()],
            frozen_tail,
            "frozen user-batch tail must survive resize"
        );
    }

    #[test]
    #[should_panic(expected = "attempted to extend user batch")]
    fn extending_user_batch_after_commit_is_rejected() {
        let mut t = TranscriptState::default();
        push_all(&mut t, user(&["a"]));
        t.ensure_render_cache(80);
        let count = t.committable_len();
        t.mark_rows_committed(count);
        // Appending a consecutive user message tries to grow an already-committed
        // batch; in a debug build this must be rejected.
        t.push_item(TimelineItem::UserMessage {
            text: "b".to_string(),
        });
    }
}
