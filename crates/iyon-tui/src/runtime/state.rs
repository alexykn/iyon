use std::collections::VecDeque;

use crate::{
    component::{Component, ComponentHandle, ComponentRegistry},
    input::InputBuffer,
    physical::PhysicalRow,
    presentation::{
        ActiveContent, FlowBoundary, HostedStream, PreparedStreamFrame, layout::compile_view,
    },
    runtime::{
        active::{ActiveTurnContent, ToolActiveStatus},
        backend::ToolUpdatePresentation,
        panel::{BottomPanelBar, SteeringQueuePanel},
    },
    transcript::{
        AssistantStream, HostedUnitRegistration, SegmentKind, TimelineItem, ToolTimelineStatus,
        TranscriptState,
    },
};
use iyon_core::ReasoningLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalizeResult {
    Nothing,
    NeedsPinnedDrain,
    HandedOff,
    FullyNative,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedActiveContent {
    pub(crate) rows: Vec<PhysicalRow>,
}

#[derive(Debug)]
pub(crate) struct HostedActiveContent {
    pub(crate) content: ActiveTurnContent,
    pub(crate) leading_gap: bool,
    pub(crate) frame: Option<PreparedActiveContent>,
}

#[derive(Debug)]
pub(crate) enum LiveTail {
    Empty,
    Active(HostedActiveContent),
    Streaming {
        hosted: HostedStream<AssistantStream>,
        frame: Option<PreparedStreamFrame>,
    },
}

impl LiveTail {
    pub(crate) fn active(&self) -> Option<&ActiveTurnContent> {
        match self {
            Self::Active(active) => Some(&active.content),
            Self::Empty | Self::Streaming { .. } => None,
        }
    }

    pub(crate) fn active_mut(&mut self) -> Option<&mut ActiveTurnContent> {
        match self {
            Self::Active(active) => Some(&mut active.content),
            Self::Empty | Self::Streaming { .. } => None,
        }
    }

    pub(crate) fn streaming_ref(&self) -> Option<&HostedStream<AssistantStream>> {
        match self {
            Self::Streaming { hosted, .. } => Some(hosted),
            Self::Empty | Self::Active(_) => None,
        }
    }

    pub(crate) fn streaming_mut(&mut self) -> Option<&mut HostedStream<AssistantStream>> {
        match self {
            Self::Streaming { hosted, .. } => Some(hosted),
            Self::Empty | Self::Active(_) => None,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) input: InputBuffer,
    pub(crate) transcript: TranscriptState,
    pub(crate) live_tail: LiveTail,
    pub(crate) pending_tool_approval: Option<PendingToolApproval>,
    deferred_conversation: VecDeque<DeferredConversationAction>,
    replaying_deferred: bool,

    pub(crate) components: ComponentRegistry,
    pub(crate) bottom_panel: BottomPanelBar,
    pub(crate) steering_panel: ComponentHandle<SteeringQueuePanel>,

    pub(crate) input_view: InputViewState,
    pub(crate) info: InfoState,

    pub(crate) exit_state: ExitState,
}

impl Default for AppState {
    fn default() -> Self {
        let mut components = ComponentRegistry::new();
        let mut bottom_panel = BottomPanelBar::new();
        let steering_panel = bottom_panel.register(&mut components, SteeringQueuePanel::new());
        bottom_panel.show(&mut components, steering_panel);
        Self {
            components,
            input: InputBuffer::default(),
            transcript: TranscriptState::default(),
            live_tail: LiveTail::Empty,
            pending_tool_approval: None,
            deferred_conversation: VecDeque::new(),
            replaying_deferred: false,
            bottom_panel,
            steering_panel,
            input_view: InputViewState::default(),
            info: InfoState::default(),
            exit_state: ExitState::default(),
        }
    }
}

impl AppState {
    pub(crate) fn append_timeline_item(&mut self, item: TimelineItem) {
        self.transcript.push_item(item);
    }

    pub(crate) fn active(&self) -> Option<&ActiveTurnContent> {
        self.live_tail.active()
    }

    pub(crate) fn active_mut(&mut self) -> Option<&mut ActiveTurnContent> {
        self.live_tail.active_mut()
    }

    pub(crate) fn streaming_ref(&self) -> Option<&HostedStream<AssistantStream>> {
        self.live_tail.streaming_ref()
    }

    pub(crate) fn streaming_mut(&mut self) -> Option<&mut HostedStream<AssistantStream>> {
        self.live_tail.streaming_mut()
    }

    pub(crate) fn prepare_assistant_frame(&mut self, width: u16, desired_commit_rows: usize) {
        if let LiveTail::Streaming { hosted, frame } = &mut self.live_tail {
            *frame = Some(hosted.prepare_frame(width, desired_commit_rows));
        }
    }

    #[cfg(test)]
    pub(crate) fn assistant_frame_ref(&self) -> Option<&PreparedStreamFrame> {
        match &self.live_tail {
            LiveTail::Streaming {
                frame: Some(frame), ..
            } => Some(frame),
            LiveTail::Empty | LiveTail::Active(_) | LiveTail::Streaming { frame: None, .. } => None,
        }
    }

    pub(crate) fn take_assistant_frame(&mut self) -> Option<PreparedStreamFrame> {
        match &mut self.live_tail {
            LiveTail::Streaming { frame, .. } => frame.take(),
            LiveTail::Empty | LiveTail::Active(_) => None,
        }
    }

    pub(crate) fn assistant_live_rows(&self) -> &[PhysicalRow] {
        match &self.live_tail {
            LiveTail::Streaming {
                frame: Some(frame), ..
            } => frame.live_rows.as_slice(),
            LiveTail::Empty | LiveTail::Active(_) | LiveTail::Streaming { frame: None, .. } => &[],
        }
    }

    pub(crate) fn active_live_rows(&self) -> &[PhysicalRow] {
        match &self.live_tail {
            LiveTail::Active(HostedActiveContent {
                frame: Some(frame), ..
            }) => frame.rows.as_slice(),
            LiveTail::Empty | LiveTail::Streaming { .. } | LiveTail::Active(_) => &[],
        }
    }

    pub(crate) fn prepare_active_frame(&mut self, width: u16) {
        if let LiveTail::Active(HostedActiveContent {
            content,
            leading_gap,
            frame,
        }) = &mut self.live_tail
        {
            let mut rows = compile_view(&content.view(), width.max(1)).rows;
            if *leading_gap {
                rows.insert(0, PhysicalRow::empty());
            }
            *frame = Some(PreparedActiveContent { rows });
        }
    }

    pub(crate) fn conversation_row_count(&self) -> usize {
        self.transcript
            .uncommitted_len()
            .saturating_add(self.assistant_live_rows().len())
            .saturating_add(self.active_live_rows().len())
    }

    pub(crate) fn seal_assistant_stream(&mut self) {
        let LiveTail::Streaming { hosted, .. } = &mut self.live_tail else {
            return;
        };
        if hosted.is_sealed() {
            return;
        }

        hosted.seal();
        let unit_id = hosted.unit_id;
        let segments = hosted.content().segments().to_vec();
        self.transcript
            .seal_hosted_assistant_unit(unit_id, segments);
    }

    pub(crate) fn finalize_sealed_assistant_stream(&mut self) -> FinalizeResult {
        let Some(hosted) = self.streaming_ref() else {
            return FinalizeResult::Nothing;
        };
        if !hosted.is_sealed() {
            return FinalizeResult::Nothing;
        }
        if !hosted.can_handoff() {
            return FinalizeResult::NeedsPinnedDrain;
        }

        let unit_id = hosted.unit_id;
        let LiveTail::Streaming { hosted, .. } =
            std::mem::replace(&mut self.live_tail, LiveTail::Empty)
        else {
            unreachable!("streaming tail disappeared during finalization")
        };
        let handoff = hosted.into_resident_handoff();

        let fully_native = handoff.source_base == handoff.source_end
            && handoff.leading_boundary != crate::presentation::LeadingBoundaryState::Pending;
        if fully_native && self.transcript.hosted_unit_is_history_head(unit_id) {
            debug_assert!(
                self.transcript.hosted_unit_is_history_head(unit_id),
                "HostedStream retired ahead of global transcript frontier"
            );
            self.transcript.finish_stream_unit(unit_id);
            self.replay_deferred_conversation();
            return FinalizeResult::FullyNative;
        }

        self.transcript.adopt_resident_stream(handoff);
        self.replay_deferred_conversation();
        FinalizeResult::HandedOff
    }

    fn attach_active_content(&mut self, content: ActiveTurnContent) {
        match &mut self.live_tail {
            LiveTail::Empty => {
                let leading_gap = content.boundary() == FlowBoundary::Default
                    && self.transcript.has_conversation_content();
                self.live_tail = LiveTail::Active(HostedActiveContent {
                    content,
                    leading_gap,
                    frame: None,
                });
            }
            LiveTail::Active(active) => {
                let leading_gap = content.boundary() == FlowBoundary::Default
                    && self.transcript.has_conversation_content();
                *active = HostedActiveContent {
                    content,
                    leading_gap,
                    frame: None,
                };
            }
            LiveTail::Streaming { .. } => {
                panic!("active content cannot replace a streaming tail");
            }
        }
    }

    fn clear_active_content(&mut self) {
        if matches!(self.live_tail, LiveTail::Active(_)) {
            self.live_tail = LiveTail::Empty;
        }
    }

    pub(crate) fn start_tool_call(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    ) {
        if self.defer_after_current_stream(DeferredConversationAction::StartToolCall {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            arguments: arguments.clone(),
        }) {
            return;
        }
        self.finish_active_turn();
        self.transcript.upsert_tool_call(
            tool_call_id,
            tool_name.clone(),
            arguments,
            ToolTimelineStatus::Running,
        );
        self.attach_active_content(ActiveTurnContent::Tool {
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
        if self.defer_after_current_stream(DeferredConversationAction::RequestToolApproval {
            approval_id,
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            arguments: arguments.clone(),
        }) {
            return;
        }
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
        self.attach_active_content(ActiveTurnContent::Tool {
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
        if self.defer_after_current_stream(DeferredConversationAction::ResolveToolApproval {
            approval_id,
            tool_call_id: tool_call_id.clone(),
            approved,
        }) {
            return;
        }
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
        if self.defer_after_current_stream(DeferredConversationAction::UpdateToolCall {
            tool_name: tool_name.clone(),
            update: update.clone(),
        }) {
            return;
        }
        let detail = format_tool_update(update);
        self.attach_active_content(ActiveTurnContent::Tool {
            tool_name,
            status: ToolActiveStatus::Running,
            detail,
        });
    }

    pub(crate) fn finish_tool_call(&mut self, tool_call_id: String, is_error: bool) {
        if self.defer_after_current_stream(DeferredConversationAction::FinishToolCall {
            tool_call_id: tool_call_id.clone(),
            is_error,
        }) {
            return;
        }
        self.transcript.finish_tool_call(&tool_call_id, is_error);
        // The tool's output is already rendered in the transcript right above the
        // input. Drop the redundant "tool x: finished/failed" banner from the active
        // pane instead of echoing it (failed results are signalled by transcript
        // styling, not a separate banner).
        self.clear_active_content();
    }

    pub(crate) fn push_tool_result(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
    ) {
        if self.defer_after_current_stream(DeferredConversationAction::PushToolResult {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            text: text.clone(),
            details: details.clone(),
            is_error,
        }) {
            return;
        }
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
        if self.defer_after_current_stream(DeferredConversationAction::UserMessage(text.clone())) {
            return;
        }
        // If there was no active host, this is still a harmless barrier call; otherwise
        // `defer_after_current_stream` already sealed it before returning false.
        self.finish_active_turn();
        // If this user message is a delivered steer, remove it from the pending queue
        // (it has now actually been sent to the agent).
        let steering = self.steering_panel;
        let _ = self
            .components
            .with_mut(steering, |panel| panel.delivered(&text));
        self.append_timeline_item(TimelineItem::UserMessage { text });
        self.start_working_pane();
    }

    pub(crate) fn enqueue_steer(&mut self, text: String) {
        let steering = self.steering_panel;
        let _ = self
            .components
            .with_mut(steering, |panel| panel.queued(text));
    }

    pub(crate) fn request_exit(&mut self) {
        if matches!(self.exit_state, ExitState::Running) {
            self.exit_state = ExitState::Requested;
        }
    }

    pub(crate) fn start_working_pane(&mut self) {
        if self.defer_conversation(DeferredConversationAction::StartWorking) {
            return;
        }
        if self.live_tail.is_empty() {
            self.attach_active_content(ActiveTurnContent::working_spinner());
        }
    }

    pub(crate) fn receive_stream_segments(&mut self, chunks: Vec<(SegmentKind, String)>) {
        let chunks: Vec<_> = chunks
            .into_iter()
            .filter(|(_, text)| !text.is_empty())
            .collect();
        if chunks.is_empty() {
            return;
        }

        if self.defer_conversation(DeferredConversationAction::StreamSegments(chunks.clone())) {
            return;
        }
        if self.streaming_ref().is_some_and(HostedStream::is_sealed) {
            self.deferred_conversation
                .push_back(DeferredConversationAction::StreamSegments(chunks));
            return;
        }

        if let Some(hosted) = self.streaming_mut() {
            for (kind, text) in chunks {
                hosted.content_mut().push_delta(kind, &text);
            }
            return;
        }

        let HostedUnitRegistration {
            id,
            leading_boundary,
        } = self.transcript.begin_hosted_assistant_unit();
        let mut hosted = HostedStream::new(id, AssistantStream::new(), leading_boundary);
        for (kind, text) in chunks {
            hosted.content_mut().push_delta(kind, &text);
        }
        // A first presented delta atomically replaces the active-turn capability
        // with the hosted stream before the next frame is prepared.
        self.live_tail = LiveTail::Streaming {
            hosted,
            frame: None,
        };
    }

    pub(crate) fn finish_active_turn(&mut self) {
        if self.defer_conversation(DeferredConversationAction::FinishActiveTurn) {
            return;
        }
        if matches!(self.live_tail, LiveTail::Active(_)) {
            self.live_tail = LiveTail::Empty;
            return;
        }
        self.seal_assistant_stream();
    }

    pub(crate) fn fail_active_turn(&mut self, message: String) {
        if self
            .defer_after_current_stream(DeferredConversationAction::FailActiveTurn(message.clone()))
        {
            return;
        }
        self.finish_active_turn();
        self.append_error_message(message);
    }

    fn conversation_barrier_active(&self) -> bool {
        self.streaming_ref().is_some_and(HostedStream::is_sealed)
    }

    fn defer_after_current_stream(&mut self, action: DeferredConversationAction) -> bool {
        if self
            .streaming_ref()
            .is_some_and(|hosted| !hosted.is_sealed())
        {
            self.seal_assistant_stream();
        }
        self.defer_conversation(action)
    }

    fn defer_conversation(&mut self, action: DeferredConversationAction) -> bool {
        if !self.conversation_barrier_active() {
            return false;
        }
        if self.replaying_deferred {
            // Keep the action being replayed ahead of events already queued
            // behind it. It will be retried after the newly sealed host drains.
            self.deferred_conversation.push_front(action);
        } else {
            self.deferred_conversation.push_back(action);
        }
        true
    }

    fn replay_deferred_conversation(&mut self) {
        self.replaying_deferred = true;
        while let Some(action) = self.deferred_conversation.pop_front() {
            if self.conversation_barrier_active() {
                self.deferred_conversation.push_front(action);
                break;
            }
            match action {
                DeferredConversationAction::UserMessage(text) => self.submit_user_message(text),
                DeferredConversationAction::StreamSegments(chunks) => {
                    self.receive_stream_segments(chunks)
                }
                DeferredConversationAction::StartWorking => self.start_working_pane(),
                DeferredConversationAction::FinishActiveTurn => self.finish_active_turn(),
                DeferredConversationAction::FailActiveTurn(message) => {
                    self.fail_active_turn(message)
                }
                DeferredConversationAction::StartToolCall {
                    tool_call_id,
                    tool_name,
                    arguments,
                } => self.start_tool_call(tool_call_id, tool_name, arguments),
                DeferredConversationAction::RequestToolApproval {
                    approval_id,
                    tool_call_id,
                    tool_name,
                    arguments,
                } => self.request_tool_approval(approval_id, tool_call_id, tool_name, arguments),
                DeferredConversationAction::ResolveToolApproval {
                    approval_id,
                    tool_call_id,
                    approved,
                } => self.resolve_tool_approval(approval_id, tool_call_id, approved),
                DeferredConversationAction::UpdateToolCall { tool_name, update } => {
                    self.update_tool_call(tool_name, update)
                }
                DeferredConversationAction::FinishToolCall {
                    tool_call_id,
                    is_error,
                } => self.finish_tool_call(tool_call_id, is_error),
                DeferredConversationAction::PushToolResult {
                    tool_call_id,
                    tool_name,
                    text,
                    details,
                    is_error,
                } => self.push_tool_result(tool_call_id, tool_name, text, details, is_error),
            }
        }
        self.replaying_deferred = false;
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

#[derive(Debug)]
enum DeferredConversationAction {
    UserMessage(String),
    StreamSegments(Vec<(SegmentKind, String)>),
    StartWorking,
    FinishActiveTurn,
    FailActiveTurn(String),
    StartToolCall {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    RequestToolApproval {
        approval_id: u64,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    },
    ResolveToolApproval {
        approval_id: u64,
        tool_call_id: String,
        approved: bool,
    },
    UpdateToolCall {
        tool_name: String,
        update: ToolUpdatePresentation,
    },
    FinishToolCall {
        tool_call_id: String,
        is_error: bool,
    },
    PushToolResult {
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
    },
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Line;

    fn conversation_rows(state: &mut AppState, width: u16) -> Vec<Line<'static>> {
        state.transcript.ensure_render_cache(width);
        state
            .transcript
            .uncommitted_rows()
            .iter()
            .cloned()
            .chain(
                state
                    .assistant_live_rows()
                    .iter()
                    .map(crate::terminal::ratatui::row_to_line),
            )
            .collect()
    }

    #[test]
    fn streaming_tail_cannot_resurrect_working_content() {
        let mut state = AppState::default();
        state.append_timeline_item(TimelineItem::UserMessage {
            text: "hello".to_string(),
        });
        state.start_working_pane();
        assert!(matches!(state.live_tail, LiveTail::Active(_)));

        state.receive_stream_segments(vec![(SegmentKind::Text, "first".to_string())]);
        assert!(matches!(state.live_tail, LiveTail::Streaming { .. }));

        state.start_working_pane();
        assert!(matches!(state.live_tail, LiveTail::Streaming { .. }));
        assert!(state.active().is_none());
    }

    #[test]
    fn active_flow_boundary_survives_predecessor_promotion() {
        let mut state = AppState::default();
        state.append_timeline_item(TimelineItem::UserMessage {
            text: "hello".to_string(),
        });
        state.start_working_pane();
        state.prepare_active_frame(80);
        let before = state.active_live_rows().to_vec();

        state.transcript.ensure_render_cache(80);
        let predecessor_rows = state.transcript.committable_len();
        assert!(predecessor_rows > 0);
        state.transcript.mark_rows_committed(predecessor_rows);
        assert!(state.transcript.uncommitted_rows().is_empty());
        state.prepare_active_frame(80);

        assert_eq!(before, state.active_live_rows());
        assert_eq!(before[0], PhysicalRow::empty());
        assert_eq!(before[1].plain_text(), "⠋ Working");
    }

    #[test]
    fn active_only_turn_termination_clears_the_live_tail() {
        let mut finished = AppState::default();
        finished.start_working_pane();
        finished.finish_active_turn();
        assert!(finished.live_tail.is_empty());

        let mut cancelled = AppState::default();
        cancelled.start_working_pane();
        cancelled.finish_active_turn();
        assert!(cancelled.live_tail.is_empty());

        let mut failed = AppState::default();
        failed.start_working_pane();
        failed.fail_active_turn("failed".to_string());
        assert!(failed.live_tail.is_empty());
        assert!(failed.streaming_ref().is_none());
    }

    #[test]
    fn hosted_stream_is_the_only_assistant_content_store() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(SegmentKind::Text, "assistant text".to_string())]);

        let hosted = state.streaming_ref().expect("host created");
        assert_eq!(
            hosted.content().segments(),
            &[crate::transcript::model::AssistantSegment::Text(
                "assistant text".to_string()
            )]
        );
        assert!(state.active().is_none());
    }

    #[test]
    fn pending_leading_boundary_survives_resident_handoff_without_row_change() {
        let mut state = AppState::default();
        state.transcript.push_item(TimelineItem::UserMessage {
            text: "user".to_string(),
        });
        state.receive_stream_segments(vec![(SegmentKind::Text, "assistant".to_string())]);
        state.prepare_assistant_frame(80, 0);
        let before = conversation_rows(&mut state, 80);

        state.finish_active_turn();
        assert_eq!(
            state.finalize_sealed_assistant_stream(),
            FinalizeResult::HandedOff
        );
        state.prepare_assistant_frame(80, 0);
        let after = conversation_rows(&mut state, 80);
        assert_eq!(before, after);
    }

    #[test]
    fn committed_leading_boundary_is_not_repeated_by_resident_tail() {
        let mut state = AppState::default();
        state.transcript.push_item(TimelineItem::UserMessage {
            text: "user".to_string(),
        });
        state.receive_stream_segments(vec![(SegmentKind::Text, "a\nb\nc".to_string())]);
        state.prepare_assistant_frame(80, 2);
        let prepared = state.take_assistant_frame().expect("first host frame");
        state
            .streaming_mut()
            .expect("host")
            .apply_commit_success(prepared.history);
        state.prepare_assistant_frame(80, 0);
        let before = conversation_rows(&mut state, 80);

        state.finish_active_turn();
        assert_eq!(
            state.finalize_sealed_assistant_stream(),
            FinalizeResult::HandedOff
        );
        state.prepare_assistant_frame(80, 0);
        let after = conversation_rows(&mut state, 80);
        assert_eq!(before, after);
    }

    #[test]
    fn hosted_unit_is_registered_at_birth_and_sealed_in_place() {
        let mut state = AppState::default();
        state.transcript.push_item(TimelineItem::UserMessage {
            text: "user".to_string(),
        });
        state.receive_stream_segments(vec![(SegmentKind::Text, "assistant".to_string())]);

        let hosted = state.streaming_ref().expect("host created");
        assert_eq!(
            hosted.leading_boundary,
            crate::presentation::LeadingBoundaryState::Pending
        );
        assert_eq!(state.transcript.test_items().len(), 2);
        assert!(matches!(
            state.transcript.test_items()[1],
            TimelineItem::AssistantMessage { ref segments } if segments.is_empty()
        ));

        state.finish_active_turn();
        assert_eq!(state.transcript.test_items().len(), 2);
        assert!(matches!(
            state.transcript.test_items()[1],
            TimelineItem::AssistantMessage { ref segments }
                if segments == &vec![crate::transcript::model::AssistantSegment::Text("assistant".to_string())]
        ));
    }

    #[test]
    fn sealed_host_defers_new_stream_until_handoff() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(SegmentKind::Text, "assistant A".to_string())]);
        let first_id = state.streaming_ref().expect("host A").unit_id;
        state.finish_active_turn();
        let first_end = state.streaming_mut().expect("host A").snapshot().source_end;

        state.receive_stream_segments(vec![(SegmentKind::Text, "assistant B".to_string())]);
        let host = state.streaming_mut().expect("host A retained");
        assert_eq!(host.unit_id, first_id);
        assert_eq!(host.snapshot().source_end, first_end);
        assert_eq!(state.deferred_conversation.len(), 1);

        state.prepare_assistant_frame(80, 10);
        let prepared = state.take_assistant_frame().expect("host A frame");
        state
            .streaming_mut()
            .expect("host A retained")
            .apply_commit_success(prepared.history);
        state.finalize_sealed_assistant_stream();

        let host_b = state.streaming_ref().expect("host B replayed");
        assert_ne!(host_b.unit_id, first_id);
        assert_eq!(
            host_b.content().segments(),
            &[crate::transcript::model::AssistantSegment::Text(
                "assistant B".to_string()
            )]
        );
        assert!(matches!(
            state.transcript.test_items()[0],
            TimelineItem::AssistantMessage { ref segments }
                if segments == &vec![crate::transcript::model::AssistantSegment::Text("assistant A".to_string())]
        ));
        assert!(matches!(
            state.transcript.test_items()[1],
            TimelineItem::AssistantMessage { ref segments } if segments.is_empty()
        ));
    }

    #[test]
    fn sealed_fitting_assistant_handoff_unblocks_next_turn() {
        let mut state = AppState::default();
        state.submit_user_message("first".to_string());
        state.receive_stream_segments(vec![(SegmentKind::Text, "short answer".to_string())]);
        state.prepare_assistant_frame(80, 0);

        // An open stream that fits does not spill merely because it is visible.
        assert_eq!(
            state.conversation_row_count(),
            state.transcript.uncommitted_len() + state.assistant_live_rows().len()
        );

        state.finish_active_turn();
        state.prepare_assistant_frame(80, 0);
        let live_rows = state.assistant_live_rows().len();
        assert!(live_rows > 0);

        // Sealing hands the immutable suffix to TranscriptState without forcing
        // those rows into native history or changing the canonical unit ID.
        assert_eq!(
            state.finalize_sealed_assistant_stream(),
            super::FinalizeResult::HandedOff
        );
        assert!(state.streaming_ref().is_none());
        state.transcript.ensure_render_cache(80);
        assert_eq!(
            state.transcript.uncommitted_rows().len(),
            state.transcript.committable_len()
        );

        state.submit_user_message("second".to_string());
        assert!(state.deferred_conversation.is_empty());
        assert!(state.transcript.test_items().iter().any(|item| {
            matches!(item, TimelineItem::UserMessage { text } if text == "second")
        }));
    }

    #[test]
    fn deferred_replay_keeps_barrier_action_ahead_of_later_tool_events() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(SegmentKind::Text, "first".to_string())]);
        state.finish_active_turn();

        state.receive_stream_segments(vec![(SegmentKind::Text, "second".to_string())]);
        state.start_tool_call(
            "tool-1".to_string(),
            "search".to_string(),
            serde_json::json!({}),
        );
        state.update_tool_call(
            "search".to_string(),
            ToolUpdatePresentation::Text("running".to_string()),
        );

        state.prepare_assistant_frame(80, 100);
        let prepared = state.take_assistant_frame().expect("first frame");
        state
            .streaming_mut()
            .expect("first host")
            .apply_commit_success(prepared.history);
        state.finalize_sealed_assistant_stream();

        assert!(matches!(
            state.deferred_conversation.front(),
            Some(DeferredConversationAction::StartToolCall { .. })
        ));
    }

    #[test]
    fn hosted_plan_failure_does_not_mutate_stream_or_live_rows() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(SegmentKind::Text, "hello **bo".to_string())]);
        state.prepare_assistant_frame(6, 1);

        let before_rows = state
            .assistant_frame_ref()
            .expect("prepared frame")
            .live_rows
            .clone();
        let (before_committed, before_source_base, before_partial) = {
            let hosted = state.streaming_mut().expect("host created");
            let snapshot = hosted.snapshot();
            (
                hosted.committed_through,
                snapshot.source_base,
                hosted.partial.clone(),
            )
        };

        // Simulate terminal failure by not applying the prepared plan.
        state.prepare_assistant_frame(6, 0);
        let after_rows = state
            .assistant_frame_ref()
            .expect("reprepared frame")
            .live_rows
            .clone();
        let (after_committed, after_source_base, after_partial) = {
            let hosted = state.streaming_mut().expect("host created");
            let snapshot = hosted.snapshot();
            (
                hosted.committed_through,
                snapshot.source_base,
                hosted.partial.clone(),
            )
        };

        assert_eq!(before_rows, after_rows);
        assert_eq!(before_committed, after_committed);
        assert_eq!(before_source_base, after_source_base);
        assert_eq!(before_partial, after_partial);
    }

    #[test]
    fn hosted_success_reprepares_from_compacted_source_boundary() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(SegmentKind::Text, "hello **bo".to_string())]);
        state.prepare_assistant_frame(6, 1);
        let prepared = state.take_assistant_frame().expect("prepared frame");
        state
            .streaming_mut()
            .expect("host created")
            .apply_commit_success(prepared.history);

        state.prepare_assistant_frame(80, 0);
        let snapshot = state.streaming_mut().expect("host created").snapshot();
        assert_eq!(
            snapshot.source_base,
            crate::presentation::StreamOffset::new(6)
        );
        assert_eq!(
            snapshot.view.nodes[0].owned_range(),
            crate::presentation::StreamRange::new(
                crate::presentation::StreamOffset::new(6),
                crate::presentation::StreamOffset::new(10),
            )
        );

        state.receive_stream_segments(vec![(SegmentKind::Text, "ld**".to_string())]);
        let snapshot = state.streaming_mut().expect("host created").snapshot();
        assert_eq!(
            snapshot.view.nodes[0].owned_range(),
            crate::presentation::StreamRange::new(
                crate::presentation::StreamOffset::new(6),
                crate::presentation::StreamOffset::new(14),
            )
        );
    }

    #[test]
    fn sealing_hands_off_before_full_history_promotion() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(SegmentKind::Text, "hello".to_string())]);
        state.finish_active_turn();

        let hosted = state.streaming_ref().expect("host retained");
        assert!(hosted.is_sealed());
        assert!(hosted.can_handoff());

        state.prepare_assistant_frame(80, 10);
        let prepared = state.take_assistant_frame().expect("prepared frame");
        state
            .streaming_mut()
            .expect("host retained")
            .apply_commit_success(prepared.history);
        assert!(state.streaming_ref().unwrap().can_handoff());
        state.finalize_sealed_assistant_stream();
        assert!(state.streaming_ref().is_none());
    }

    #[test]
    fn resident_stream_preserves_birth_unit_id() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(SegmentKind::Text, "hello".to_string())]);
        let id = state.streaming_ref().expect("host created").unit_id;
        state.finish_active_turn();
        state.prepare_assistant_frame(80, 10);
        let prepared = state.take_assistant_frame().expect("prepared frame");
        state
            .streaming_mut()
            .expect("host retained")
            .apply_commit_success(prepared.history);
        state.finalize_sealed_assistant_stream();

        assert_eq!(state.transcript.test_entry_ids(), &[id]);
    }

    /// A steered message submitted while the agent is mid-stream seals the assistant
    /// host and defers the user until the hosted tail has handed off.
    #[test]
    fn submit_user_message_finalizes_active_assistant() {
        let mut state = AppState::default();

        // Stream a partial assistant reply into the active pane.
        state.receive_stream_segments(vec![(SegmentKind::Text, "partial reply".to_string())]);

        // Steer while streaming.
        state.submit_user_message("steer message".to_string());

        let items = state.transcript.test_items();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], TimelineItem::AssistantMessage { .. }));
        assert!(
            state
                .streaming_ref()
                .is_some_and(|hosted| hosted.is_sealed())
        );
        // The user and fresh spinner are deferred until the sealed assistant tail
        // has become an immutable resident transcript unit.
        assert!(state.active().is_none());

        state.prepare_assistant_frame(80, 10);
        let prepared = state.take_assistant_frame().expect("prepared frame");
        state
            .streaming_mut()
            .expect("sealed host")
            .apply_commit_success(prepared.history);
        state.finalize_sealed_assistant_stream();

        let items = state.transcript.test_items();
        assert_eq!(items.len(), 2);
        assert!(matches!(items[1], TimelineItem::UserMessage { .. }));
        assert!(matches!(
            state.active(),
            Some(ActiveTurnContent::WorkingSpinner { .. })
        ));
    }

    /// A steered message stays in the pending queue until it is actually delivered to
    /// the agent; `submit_user_message` (the delivery path) removes it and appends it
    /// to the transcript.
    #[test]
    fn submit_user_message_removes_delivered_steer_from_queue() {
        let mut state = AppState::default();
        state.enqueue_steer("hello".to_string());
        assert_eq!(state.bottom_panel.desired_height(&state.components, 80), 1);

        state.submit_user_message("hello".to_string());

        assert_eq!(state.bottom_panel.desired_height(&state.components, 80), 0);
        let items = state.transcript.test_items();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], TimelineItem::UserMessage { .. }));
    }
}
