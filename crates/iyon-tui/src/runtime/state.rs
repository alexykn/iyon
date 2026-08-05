use crate::{
    input::InputBuffer,
    runtime::{
        active::{ActivePaneState, ToolActiveStatus},
        backend::ToolUpdatePresentation,
    },
    transcript::{AssistantSegment, SegmentKind, TimelineItem, ToolTimelineStatus, TranscriptState},
};
use iyon_core::ReasoningLevel;

#[derive(Debug, Default)]
pub(crate) struct AppState {
    pub(crate) input: InputBuffer,
    pub(crate) transcript: TranscriptState,
    pub(crate) active: Option<ActivePaneState>,
    pub(crate) pending_tool_approval: Option<PendingToolApproval>,

    pub(crate) input_view: InputViewState,
    pub(crate) info: InfoState,

    pub(crate) exit_state: ExitState,
}

impl AppState {
    pub(crate) fn append_timeline_item(&mut self, item: TimelineItem) {
        self.transcript.push_item(item);
    }

    pub(crate) fn append_assistant_message(&mut self, segments: Vec<AssistantSegment>) {
        self.transcript.append_assistant_fragment(segments);
    }

    pub(crate) fn append_assistant_stream_fragment(&mut self, segments: Vec<AssistantSegment>) {
        self.transcript.append_assistant_stream_fragment(segments);
    }

    pub(crate) fn finish_assistant_stream(&mut self) {
        self.transcript.finish_assistant_stream();
    }

    pub(crate) fn start_tool_call(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    ) {
        self.finish_active_turn();
        self.transcript.upsert_tool_call(
            tool_call_id,
            tool_name.clone(),
            arguments,
            ToolTimelineStatus::Running,
        );
        self.active = Some(ActivePaneState::Tool {
            tool_name,
            status: ToolActiveStatus::Running,
            detail: None,
        });
    }

    pub(crate) fn request_tool_approval(
        &mut self,
        approval_id: u64,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    ) {
        let arguments_preview = format_arguments_preview(&arguments);
        self.transcript.upsert_tool_call(
            tool_call_id.clone(),
            tool_name.clone(),
            arguments,
            ToolTimelineStatus::PendingApproval,
        );
        self.pending_tool_approval = Some(PendingToolApproval {
            approval_id,
            tool_call_id,
            tool_name: tool_name.clone(),
        });
        self.active = Some(ActivePaneState::Tool {
            tool_name,
            status: ToolActiveStatus::WaitingForApproval { approval_id },
            detail: Some(arguments_preview),
        });
    }

    pub(crate) fn resolve_tool_approval(
        &mut self,
        approval_id: u64,
        tool_call_id: String,
        approved: bool,
    ) {
        let tool_name = self
            .pending_tool_approval
            .as_ref()
            .filter(|pending| pending.approval_id == approval_id)
            .map(|pending| pending.tool_name.clone())
            .unwrap_or_default();
        if self
            .pending_tool_approval
            .as_ref()
            .is_some_and(|pending| pending.approval_id == approval_id)
        {
            self.pending_tool_approval = None;
        }
        let status = if approved {
            ToolTimelineStatus::Approved
        } else {
            ToolTimelineStatus::Rejected
        };
        self.transcript
            .update_tool_call_status(tool_call_id, tool_name, status);
    }

    pub(crate) fn update_tool_call(&mut self, tool_name: String, update: ToolUpdatePresentation) {
        let detail = format_tool_update(update);
        self.active = Some(ActivePaneState::Tool {
            tool_name,
            status: ToolActiveStatus::Running,
            detail,
        });
    }

    pub(crate) fn finish_tool_call(&mut self, tool_call_id: String, is_error: bool) {
        self.transcript.finish_tool_call(&tool_call_id, is_error);
        if let Some(ActivePaneState::Tool { status, .. }) = self.active.as_mut() {
            *status = ToolActiveStatus::Finished { is_error };
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
        self.transcript
            .push_tool_result(tool_call_id, tool_name, text, details, is_error);
    }

    pub(crate) fn append_error_message(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.transcript
            .push_item(TimelineItem::ErrorMessage { text });
    }

    pub(crate) fn submit_user_message(&mut self, text: String) {
        self.append_timeline_item(TimelineItem::UserMessage { text });
        self.start_working_pane();
    }

    pub(crate) fn request_exit(&mut self) {
        if matches!(self.exit_state, ExitState::Running) {
            self.exit_state = ExitState::Requested;
        }
    }

    pub(crate) fn start_working_pane(&mut self) {
        if self.active.is_none() {
            self.active = Some(ActivePaneState::working_spinner());
        }
    }

    pub(crate) fn receive_stream_segments(&mut self, chunks: Vec<(SegmentKind, String)>) {
        for (kind, text) in chunks {
            if !self
                .active
                .as_ref()
                .is_some_and(super::active::ActivePaneState::is_spillable)
            {
                self.active = Some(ActivePaneState::working_spinner());
            }

            if let Some(active) = self.active.as_mut() {
                active.push_assistant_segment(kind, &text);
            }
        }
    }

    pub(crate) fn take_active_unfrozen_transcript_segments(
        &mut self,
    ) -> Option<Vec<AssistantSegment>> {
        self.active
            .take()
            .and_then(ActivePaneState::into_unfrozen_transcript_segments)
    }

    pub(crate) fn finish_active_turn(&mut self) {
        if let Some(segments) = self.take_active_unfrozen_transcript_segments() {
            self.append_assistant_stream_fragment(segments);
        }
        self.finish_assistant_stream();
    }

    pub(crate) fn fail_active_turn(&mut self, message: String) {
        self.finish_active_turn();
        self.append_error_message(message);
    }

    pub(crate) fn apply_config(
        &mut self,
        provider: String,
        model_id: String,
        reasoning_effort: ReasoningLevel,
    ) {
        self.info.provider = provider;
        self.info.model_id = model_id;
        self.info.reasoning_effort = reasoning_effort;
    }

    /// Optimistically advances the displayed reasoning effort to the next
    /// provider-supported level using the same shared step the core applies.
    /// Purely presentational; the core remains authoritative and re-anchors
    /// this value via `apply_config` on the next `ConfigChanged` round-trip.
    pub(crate) fn cycle_reasoning_effort(&mut self) {
        let provider = self.info.provider.as_str();
        self.info.reasoning_effort = ReasoningLevel::next_for(self.info.reasoning_effort, provider);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingToolApproval {
    pub(crate) approval_id: u64,
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
}

fn format_arguments_preview(value: &serde_json::Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    truncate_preview(text)
}

fn format_tool_update(update: ToolUpdatePresentation) -> Option<String> {
    let text = match update {
        ToolUpdatePresentation::Text(text) => text,
        ToolUpdatePresentation::Progress {
            label,
            current,
            total,
        } => match (current, total) {
            (Some(current), Some(total)) => format!("{label}: {current}/{total}"),
            (Some(current), None) => format!("{label}: {current}"),
            (None, Some(total)) => format!("{label}: 0/{total}"),
            (None, None) => label,
        },
        ToolUpdatePresentation::Details(details) => details.to_string(),
    };
    (!text.is_empty()).then(|| truncate_preview(text))
}

fn truncate_preview(mut text: String) -> String {
    const MAX_CHARS: usize = 1000;
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    text = text.chars().take(MAX_CHARS).collect();
    text.push('…');
    text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExitState {
    #[default]
    Running,
    Requested,
    FinalizeActive,
    FlushHistory,
    FinalFrame,
    Done,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct InputViewState {
    pub(crate) content_width: u16,
    pub(crate) scroll_rows: u16,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InfoState {
    pub(crate) status: String,
    pub(crate) provider: String,
    pub(crate) model_id: String,
    pub(crate) reasoning_effort: ReasoningLevel,
}
