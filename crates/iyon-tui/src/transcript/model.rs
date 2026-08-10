use std::{borrow::Cow, collections::HashMap, ops::Range};

use crate::{
    history::FlowBoundary,
    physical::PhysicalRow,
    presentation::{
        ColorSpec, Insets, IntoView, OverflowIndicator, ThemeKey, View, layout::compile_view,
    },
    stream::FrozenPhysicalRows,
    tools::{ToolCallRenderInput, ToolOutcome, ToolRendererRegistry, ToolResultRenderInput},
    transcript::markdown::{assistant_document_view, parse_assistant},
    transcript::{LeadingBoundaryState, ResidentStreamHandoff},
};

/// The kind of an assistant-message segment. `Thinking` is streamed reasoning,
/// kept logically distinct from answer text so it can be styled independently.
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
            Self::Text(text) | Self::Thinking(text) => text,
        }
    }
}

/// Returns the sub-range of `segments` covering bytes `[from_byte, to_byte)`.
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

        let start = clamp_to_char_boundary(text, from.saturating_sub(cursor));
        let end = char_boundary_after(text, to.saturating_sub(cursor)).max(start);
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

fn char_boundary_after(text: &str, mut pos: usize) -> usize {
    pos = pos.min(text.len());
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    pos
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

/// A semantic resident suffix transferred from the hosted stream owner.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResidentStreamTail {
    pub(crate) view: View,
    pub(crate) boundary: FlowBoundary,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PresentationOverride {
    Hosted { sealed: bool },
    Resident(ResidentStreamTail),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentationOwnership {
    Transcript,
    Hosted,
    ResidentStream,
    NativeHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptId(pub(crate) u64);

pub(crate) type TranscriptUnitId = TranscriptId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryLifecycle {
    Open,
    Sealed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryCommitMode {
    Sealed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostedUnitRegistration {
    pub(crate) id: TranscriptUnitId,
    pub(crate) leading_boundary: LeadingBoundaryState,
}

/// One durable transcript unit. Resident presentation is always a semantic
/// `View`; ownership determines whether the View is currently resident.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptUnit {
    pub(crate) id: TranscriptUnitId,
    pub(crate) source_ids: Vec<TranscriptId>,
    pub(crate) view: Option<View>,
    pub(crate) ownership: PresentationOwnership,
    pub(crate) boundary: FlowBoundary,
    pub(crate) lifecycle: EntryLifecycle,
    commit_mode: EntryCommitMode,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TuiFormatter {
    tool_renderers: ToolRendererRegistry,
    pub(crate) show_arg_preview: bool,
}

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> View {
        match item {
            TimelineItem::UserMessage { text } => Self::user_batch_view(std::slice::from_ref(text)),
            TimelineItem::AssistantMessage { segments } => {
                assistant_document_view(&parse_assistant(segments))
            }
            TimelineItem::ErrorMessage { text } => self.format_error_message(text),
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

    fn user_batch_view(messages: &[String]) -> View {
        View::vertical(|column| {
            column.children(
                messages
                    .iter()
                    .map(|text| View::text(text.clone()).fill_width()),
            );
        })
        .fill_width()
        .padding(Insets::new(1, 0, 1, 0))
        .background(ColorSpec::Theme(ThemeKey::from("surface.user")))
    }

    fn format_error_message(&self, text: &str) -> View {
        View::text(text)
            .fill_width()
            .style(
                crate::presentation::StyleSpec::new()
                    .foreground(ColorSpec::Theme(ThemeKey::from("text.error"))),
            )
            .into_view()
    }

    fn format_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        status: ToolTimelineStatus,
    ) -> View {
        let view = self.tool_renderers.render_call(ToolCallRenderInput {
            tool_name,
            arguments,
            status,
            show_arg_preview: self.show_arg_preview,
        });
        if self.show_arg_preview {
            view.clamp_rows(
                17,
                OverflowIndicator::Footer {
                    prefix: "… more argument lines".to_string(),
                    style: truncation_footer_style_spec(),
                },
            )
        } else {
            view
        }
    }

    fn format_tool_result(
        &self,
        tool_name: &str,
        text: &str,
        details: &serde_json::Value,
        is_error: bool,
        collapsed: bool,
    ) -> View {
        let view = self.tool_renderers.render_result(ToolResultRenderInput {
            tool_name,
            text,
            details,
            outcome: if is_error {
                ToolOutcome::Error
            } else {
                ToolOutcome::Success
            },
        });
        if collapsed {
            view.clamp_rows(
                16,
                OverflowIndicator::Footer {
                    prefix: "… more lines (full result retained)".to_string(),
                    style: truncation_footer_style_spec(),
                },
            )
        } else {
            view
        }
    }
}

fn truncation_footer_style_spec() -> crate::presentation::StyleSpec {
    crate::presentation::StyleSpec::new()
        .foreground(ColorSpec::Theme(ThemeKey::from("truncation_footer")))
        .italic()
        .dim()
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
            overrides: HashMap::new(),
        }
    }
}

impl TranscriptState {
    pub(crate) fn has_conversation_content(&self) -> bool {
        !self.presentation_cache.is_empty()
    }

    pub(crate) fn push_item(&mut self, item: TimelineItem) {
        if matches!(&item, TimelineItem::UserMessage { .. })
            && self
                .canonical_items
                .last()
                .is_some_and(|last| matches!(last, TimelineItem::UserMessage { .. }))
            && let Some(&last_id) = self.entry_ids.last()
        {
            self.assert_unit_extendable(last_id);
        }
        self.canonical_items.push(item);
        let id = self.allocate_id();
        self.entry_ids.push(id);
        self.rebuild_presentation_cache();
    }

    fn allocate_id(&mut self) -> TranscriptId {
        let id = TranscriptId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn unit_index_for_source(&self, source_id: TranscriptId) -> Option<usize> {
        self.presentation_cache
            .iter()
            .position(|unit| unit.source_ids.contains(&source_id))
    }

    fn assert_unit_extendable(&self, source_id: TranscriptId) {
        let Some(index) = self.unit_index_for_source(source_id) else {
            return;
        };
        let id = self.presentation_cache[index].id;
        debug_assert!(
            index >= self.commit_state.completed_prefix
                && !self
                    .commit_state
                    .partial
                    .as_ref()
                    .is_some_and(|partial| partial.entry_id() == id),
            "attempted to extend user batch {id:?} after native history commitment"
        );
    }

    fn assert_source_mutable(&self, source_id: TranscriptId) {
        let Some(index) = self.unit_index_for_source(source_id) else {
            return;
        };
        let id = self.presentation_cache[index].id;
        debug_assert!(
            index >= self.commit_state.completed_prefix,
            "attempted to mutate fully committed transcript unit {id:?}"
        );
        debug_assert!(
            !self
                .commit_state
                .partial
                .as_ref()
                .is_some_and(|partial| partial.entry_id() == id),
            "attempted to mutate partially committed transcript unit {id:?}"
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
                .insert(id, PresentationOverride::Hosted { sealed: false })
                .is_none(),
            "hosted transcript unit cannot be registered twice"
        );
        self.rebuild_presentation_cache();
        HostedUnitRegistration {
            id,
            leading_boundary,
        }
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
        let Some(PresentationOverride::Hosted { sealed }) = self.overrides.get_mut(&unit_id) else {
            panic!("only a hosted unit can be sealed");
        };
        *sealed = true;
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
            Some(PresentationOverride::Hosted { .. })
        ) && index == self.commit_state.completed_prefix
            && self.commit_state.partial.is_none()
    }

    pub(crate) fn hosted_unit_is_history_tail(&self, unit_id: TranscriptUnitId) -> bool {
        self.presentation_cache.last().is_some_and(|unit| {
            unit.id == unit_id
                && matches!(
                    self.overrides.get(&unit_id),
                    Some(PresentationOverride::Hosted { .. })
                )
        })
    }

    pub(crate) fn adopt_resident_stream(&mut self, handoff: ResidentStreamHandoff) {
        let unit_index = self
            .presentation_cache
            .iter()
            .position(|unit| unit.id == handoff.unit_id)
            .expect("resident handoff unit must exist");
        assert!(
            matches!(
                self.presentation_cache[unit_index].ownership,
                PresentationOwnership::Hosted
            ),
            "resident handoff must consume a hosted unit"
        );
        let source_id = self.presentation_cache[unit_index]
            .source_ids
            .first()
            .copied()
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

        self.overrides.insert(
            handoff.unit_id,
            PresentationOverride::Resident(ResidentStreamTail {
                view: handoff.view,
                boundary: match handoff.leading_boundary {
                    LeadingBoundaryState::Pending => self.presentation_cache[unit_index].boundary,
                    LeadingBoundaryState::Committed | LeadingBoundaryState::None => {
                        FlowBoundary::AttachToPrevious
                    }
                },
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
                Some(PresentationOverride::Hosted { sealed: true })
            ),
            "stream unit must be hosted before native retirement"
        );
        assert_eq!(
            unit_index, self.commit_state.completed_prefix,
            "hosted unit cannot retire ahead of global history frontier"
        );
        assert!(self.commit_state.partial.is_none());
        self.overrides.remove(&unit_id);
        self.commit_state.completed_prefix = unit_index.saturating_add(1);
        self.rebuild_presentation_cache();
        self.invalidate_render_cache();
    }

    pub(crate) fn upsert_tool_call(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
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

    #[cfg(test)]
    pub(crate) fn test_items(&self) -> &[TimelineItem] {
        &self.canonical_items
    }

    #[cfg(test)]
    pub(crate) fn test_entry_ids(&self) -> &[TranscriptId] {
        &self.entry_ids
    }

    fn ownership_for(&self, unit_id: TranscriptUnitId, unit_index: usize) -> PresentationOwnership {
        if unit_index < self.commit_state.completed_prefix {
            return PresentationOwnership::NativeHistory;
        }
        match self.overrides.get(&unit_id) {
            Some(PresentationOverride::Hosted { .. }) => PresentationOwnership::Hosted,
            Some(PresentationOverride::Resident(_)) => PresentationOwnership::ResidentStream,
            None => PresentationOwnership::Transcript,
        }
    }

    fn rebuild_presentation_cache(&mut self) {
        self.presentation_cache.clear();
        let mut index = 0usize;
        while index < self.canonical_items.len() {
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
                    view: Some(TuiFormatter::user_batch_view(&messages)),
                    ownership: PresentationOwnership::Transcript,
                    boundary: FlowBoundary::Default,
                    lifecycle: EntryLifecycle::Sealed,
                    commit_mode: EntryCommitMode::Sealed,
                });
                continue;
            }

            let item = &self.canonical_items[index];
            let source_id = self.entry_ids[index];
            let unit_index = self.presentation_cache.len();
            let ownership = self.ownership_for(source_id, unit_index);
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
            let boundary = match self.overrides.get(&source_id) {
                Some(PresentationOverride::Resident(tail)) => tail.boundary,
                _ => boundary,
            };
            let view = match &ownership {
                PresentationOwnership::Transcript => Some(self.formatter.format(item)),
                PresentationOwnership::ResidentStream => match self.overrides.get(&source_id) {
                    Some(PresentationOverride::Resident(tail)) => Some(tail.view.clone()),
                    _ => unreachable!("resident stream presentation must exist"),
                },
                PresentationOwnership::Hosted | PresentationOwnership::NativeHistory => None,
            };
            let lifecycle = match self.overrides.get(&source_id) {
                Some(PresentationOverride::Hosted { sealed: false }) => EntryLifecycle::Open,
                _ => entry_lifecycle(item),
            };
            let commit_mode = match ownership {
                PresentationOwnership::Hosted => EntryCommitMode::Blocked,
                PresentationOwnership::Transcript
                | PresentationOwnership::ResidentStream
                | PresentationOwnership::NativeHistory => entry_commit_mode(item),
            };
            self.presentation_cache.push(TranscriptUnit {
                id: source_id,
                source_ids: vec![source_id],
                view,
                ownership,
                boundary,
                lifecycle,
                commit_mode,
            });
            index += 1;
        }
        self.invalidate_render_cache();
    }

    fn invalidate_render_cache(&mut self) {
        if let Some(cache) = self.rendered_rows_cache.as_mut() {
            cache.width = None;
        }
    }

    fn semantic_view_for(unit: &TranscriptUnit, has_predecessor: bool) -> Option<View> {
        let view = unit.view.clone()?;
        if has_predecessor && unit.boundary == FlowBoundary::Default {
            Some(
                View::vertical(|column| {
                    column.child(View::spacer(1));
                    column.child(view);
                })
                .fill_width(),
            )
        } else {
            Some(view)
        }
    }

    fn render_semantic(&self, width: u16) -> TranscriptRenderCache {
        let mut rows = Vec::new();
        let mut ranges = Vec::new();
        let partial = self.commit_state.partial.as_ref();

        for (unit_index, unit) in self
            .presentation_cache
            .iter()
            .enumerate()
            .skip(self.commit_state.completed_prefix)
        {
            let Some(view) = Self::semantic_view_for(unit, unit_index > 0) else {
                break;
            };
            let start = rows.len();
            let (unit_rows, transferable) = match partial {
                Some(PartialCommit {
                    entry_id,
                    rows: frozen,
                    committed_rows,
                }) if *entry_id == unit.id => {
                    let remaining = frozen.as_slice().get(*committed_rows..).unwrap_or(&[]);
                    (remaining.to_vec(), remaining.len())
                }
                _ => {
                    let block = compile_view(&view, width);
                    let transferable = block
                        .physically_complete
                        .then_some(block.rows.len())
                        .unwrap_or(0);
                    (block.rows, transferable)
                }
            };
            rows.extend(unit_rows);
            ranges.push(RenderedEntryRange {
                unit_index,
                id: unit.id,
                commit_mode: unit.commit_mode,
                rows: start..rows.len(),
                transferable_prefix_rows: transferable,
            });
        }

        let mut committable_rows: usize = 0;
        for range in &ranges {
            if range.commit_mode == EntryCommitMode::Blocked {
                break;
            }
            committable_rows = committable_rows.saturating_add(range.transferable_prefix_rows);
            if range.transferable_prefix_rows < range.rows.len() {
                break;
            }
        }
        TranscriptRenderCache {
            width: Some(width),
            rows,
            ranges,
            committable_rows,
        }
    }

    pub(crate) fn ensure_render_cache(&mut self, width: u16) {
        if self
            .rendered_rows_cache
            .as_ref()
            .is_some_and(|cache| cache.width == Some(width))
        {
            return;
        }
        self.rendered_rows_cache = Some(self.render_semantic(width));
    }

    pub(crate) fn committable_len(&self) -> usize {
        self.rendered_rows_cache
            .as_ref()
            .map_or(0, |cache| cache.committable_rows)
    }

    pub(crate) fn uncommitted_rows(&self) -> &[PhysicalRow] {
        self.rendered_rows_cache
            .as_ref()
            .map_or(&[], |cache| cache.rows.as_slice())
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
            let partial = match self.commit_state.partial.take() {
                Some(PartialCommit {
                    entry_id,
                    rows,
                    committed_rows,
                }) if entry_id == range.id => PartialCommit {
                    entry_id,
                    rows,
                    committed_rows: committed_rows.saturating_add(take),
                },
                _ => PartialCommit {
                    entry_id: range.id,
                    rows: FrozenPhysicalRows::new(cache.rows[range.rows.clone()].to_vec()),
                    committed_rows: take,
                },
            };
            self.commit_state.partial = Some(partial);

            if take == range_len {
                self.commit_state.partial = None;
                if matches!(
                    self.overrides.get(&range.id),
                    Some(PresentationOverride::Resident(_))
                ) {
                    self.overrides.remove(&range.id);
                }
                self.commit_state.completed_prefix = range.unit_index.saturating_add(1);
                cache.ranges.remove(0);
            } else {
                break;
            }
            remaining -= take;
        }

        cache.rows.drain(..committed_rows);
        for range in &mut cache.ranges {
            range.rows.start = range.rows.start.saturating_sub(committed_rows);
            range.rows.end = range.rows.end.saturating_sub(committed_rows);
        }
        cache.committable_rows = cache.committable_rows.saturating_sub(committed_rows);
        cache.width = None;
        self.rendered_rows_cache = Some(cache);
    }
}

#[derive(Debug, Clone)]
struct TranscriptCommitState {
    completed_prefix: usize,
    partial: Option<PartialCommit>,
}

#[derive(Debug, Clone)]
struct PartialCommit {
    entry_id: TranscriptId,
    rows: FrozenPhysicalRows,
    committed_rows: usize,
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
        self.entry_id
    }
}

fn entry_commit_mode(item: &TimelineItem) -> EntryCommitMode {
    match item {
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

fn entry_lifecycle(item: &TimelineItem) -> EntryLifecycle {
    match item {
        TimelineItem::ToolCall { status, .. } => match status {
            ToolTimelineStatus::Finished
            | ToolTimelineStatus::Failed
            | ToolTimelineStatus::Rejected => EntryLifecycle::Sealed,
            _ => EntryLifecycle::Open,
        },
        _ => EntryLifecycle::Sealed,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TranscriptRenderCache {
    width: Option<u16>,
    rows: Vec<PhysicalRow>,
    ranges: Vec<RenderedEntryRange>,
    committable_rows: usize,
}

#[derive(Debug, Clone)]
struct RenderedEntryRange {
    unit_index: usize,
    id: TranscriptUnitId,
    commit_mode: EntryCommitMode,
    rows: Range<usize>,
    transferable_prefix_rows: usize,
}

/// The single semantic thinking-to-answer separator used by both transcript
/// assembly and AssistantStream.
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
        s.push_str("\n\n");
        s.push_str(chunk);
        Cow::Owned(s)
    } else {
        Cow::Borrowed(chunk)
    }
}

fn push_segment(target: &mut Vec<AssistantSegment>, segment: AssistantSegment) {
    let kind = match &segment {
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
    use crate::{history::History, transcript::AssistantStream};

    #[test]
    fn hosted_and_native_ownership_have_no_resident_view() {
        let mut state = TranscriptState::default();
        let registration = state.begin_hosted_assistant_unit();
        assert!(state.presentation_cache[0].view.is_none());
        assert_eq!(
            state.presentation_cache[0].ownership,
            PresentationOwnership::Hosted
        );
        state.seal_hosted_assistant_unit(registration.id, Vec::new());
        state.finish_stream_unit(registration.id);
        assert_eq!(
            state.presentation_cache[0].ownership,
            PresentationOwnership::NativeHistory
        );
        assert!(state.presentation_cache[0].view.is_none());
    }

    #[test]
    fn durable_units_are_history_compatible() {
        let mut state = TranscriptState::default();
        state.push_item(TimelineItem::UserMessage {
            text: "user".into(),
        });
        state.push_item(TimelineItem::ErrorMessage {
            text: "error".into(),
        });
        let mut history = History::new();
        for unit in &state.presentation_cache {
            history
                .push_with_boundary(unit.view.clone().expect("durable View"), unit.boundary)
                .expect("semantic durable unit is History-compatible");
        }
    }

    #[test]
    fn durable_presentation_cases_are_semantic_views() {
        let items = [
            TimelineItem::UserMessage {
                text: "user".into(),
            },
            TimelineItem::AssistantMessage {
                segments: vec![AssistantSegment::Text("assistant".into())],
            },
            TimelineItem::ErrorMessage {
                text: "error".into(),
            },
            TimelineItem::ToolCall {
                tool_call_id: "call".into(),
                tool_name: "read".into(),
                arguments: serde_json::Value::Null,
                status: ToolTimelineStatus::Finished,
            },
            TimelineItem::ToolResult {
                tool_call_id: "call".into(),
                tool_name: "read".into(),
                text: "result".into(),
                details: serde_json::Value::Null,
                is_error: false,
                collapsed: true,
            },
        ];
        let mut state = TranscriptState::default();
        for item in items {
            state.push_item(item);
        }
        assert!(state.presentation_cache.iter().all(|unit| {
            unit.ownership == PresentationOwnership::Transcript && unit.view.is_some()
        }));
    }

    #[test]
    fn default_flow_is_compiled_as_semantic_geometry() {
        let mut state = TranscriptState::default();
        state.push_item(TimelineItem::ErrorMessage { text: "a".into() });
        state.push_item(TimelineItem::ErrorMessage { text: "b".into() });
        state.ensure_render_cache(80);
        assert_eq!(state.uncommitted_rows().len(), 3);
        assert_eq!(state.uncommitted_rows()[1].plain_text(), "");
        assert_eq!(state.uncommitted_rows()[2].plain_text(), "b");
    }

    #[test]
    fn native_predecessor_keeps_first_resident_default_gap() {
        let mut state = TranscriptState::default();
        state.push_item(TimelineItem::ErrorMessage {
            text: "first".into(),
        });
        state.ensure_render_cache(80);
        let first_rows = state.committable_len();
        state.mark_rows_committed(first_rows);
        state.push_item(TimelineItem::ErrorMessage {
            text: "resident".into(),
        });
        state.ensure_render_cache(80);
        assert_eq!(state.uncommitted_rows()[0].plain_text(), "");
        assert_eq!(state.uncommitted_rows()[1].plain_text(), "resident");
    }

    #[test]
    fn attach_to_previous_has_no_default_flow_gap() {
        let mut state = TranscriptState::default();
        state.upsert_tool_call(
            "call".into(),
            "read".into(),
            serde_json::Value::Null,
            ToolTimelineStatus::Finished,
        );
        state.push_tool_result(
            "call".into(),
            "read".into(),
            "result".into(),
            serde_json::Value::Null,
            false,
        );
        assert_eq!(
            state.presentation_cache[1].boundary,
            FlowBoundary::AttachToPrevious
        );
        state.ensure_render_cache(80);
        assert!(
            !state
                .uncommitted_rows()
                .iter()
                .any(|row| row.plain_text().is_empty())
        );
    }

    #[test]
    fn zero_width_has_no_fabricated_physical_content() {
        let mut state = TranscriptState::default();
        state.push_item(TimelineItem::ErrorMessage {
            text: "content".into(),
        });
        state.ensure_render_cache(0);
        assert!(state.uncommitted_rows().iter().all(|row| row.width() == 0));
        assert_eq!(state.committable_len(), 0);
    }

    #[test]
    fn collapsed_result_uses_semantic_clamp_footer() {
        let mut state = TranscriptState::default();
        state.push_tool_result(
            "call".into(),
            "read".into(),
            (0..40)
                .map(|i| format!("line{i}"))
                .collect::<Vec<_>>()
                .join("\n"),
            serde_json::Value::Null,
            false,
        );
        state.ensure_render_cache(80);
        assert_eq!(state.uncommitted_rows().len(), 16);
        assert!(
            state
                .uncommitted_rows()
                .last()
                .is_some_and(|row| row.plain_text().contains("more lines"))
        );
    }

    #[test]
    fn tool_call_preview_uses_semantic_clamp_when_enabled() {
        let formatter = TuiFormatter {
            show_arg_preview: true,
            ..TuiFormatter::default()
        };
        let view = formatter.format(&TimelineItem::ToolCall {
            tool_call_id: "call".into(),
            tool_name: "unknown".into(),
            arguments: serde_json::json!({
                "content": (0..40).map(|i| format!("line{i}")).collect::<Vec<_>>()
            }),
            status: ToolTimelineStatus::Finished,
        });
        let rows = compile_view(&view, 80).rows;
        assert_eq!(rows.len(), 17);
        assert!(
            rows.last()
                .is_some_and(|row| row.plain_text().contains("argument lines"))
        );
    }

    #[test]
    fn partial_semantic_rows_are_frozen_across_resize() {
        let mut state = TranscriptState::default();
        state.push_item(TimelineItem::ErrorMessage {
            text: "abcdefghijk".into(),
        });
        state.ensure_render_cache(8);
        let original = state.uncommitted_rows().to_vec();
        let frozen_tail = original[1..].to_vec();
        state.mark_rows_committed(1);
        state.ensure_render_cache(80);
        assert_eq!(state.uncommitted_rows(), frozen_tail.as_slice());
    }

    #[test]
    fn resident_handoff_contains_only_semantic_suffix() {
        let mut state = TranscriptState::default();
        let registration = state.begin_hosted_assistant_unit();
        let mut stream = crate::transcript::HostedStream::new(
            registration.id,
            AssistantStream::new(),
            registration.leading_boundary,
        );
        stream.content_mut().push_delta(SegmentKind::Text, "a");
        stream.seal();
        state.seal_hosted_assistant_unit(registration.id, stream.content().segments().to_vec());
        let handoff = stream.into_resident_handoff();
        state.adopt_resident_stream(handoff);
        assert!(state.presentation_cache[0].view.is_some());
    }
}
