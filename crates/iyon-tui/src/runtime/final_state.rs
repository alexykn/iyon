use std::collections::HashMap;

use anyhow::Result;
use iyon_core::ReasoningLevel;

use crate::{
    ComponentHandle, FlowBoundary, History, HistoryStreamHandle, Scene, TextInput, View,
    component::ComponentRegistry,
    history::HistoryUnitId,
    output::{OutputRouter, RouteConflict},
    transcript::{AssistantStream, SegmentKind, TimelineItem, ToolTimelineStatus, TuiFormatter},
};

use super::{
    backend::ToolUpdatePresentation,
    controller::AppAction,
    final_components::{ActivityState, ApprovalDecision, ConversationActivity, UserBatch},
    panel::SteeringQueuePanel,
};

const MAX_COMPOSER_ROWS: u16 = 13;
const LARGE_PASTE_CHAR_THRESHOLD: usize = 1000;
const LARGE_PASTE_LINE_THRESHOLD: usize = 10;

#[derive(Debug, Default)]
pub(crate) struct ComposerPasteStore {
    entries: Vec<(String, String)>,
    next_id: u64,
}

impl ComposerPasteStore {
    pub(crate) fn display_text(&mut self, current: &str, input: &str) -> String {
        let normalized = input
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "    ");
        if normalized.chars().count() <= LARGE_PASTE_CHAR_THRESHOLD
            && normalized.split('\n').count() <= LARGE_PASTE_LINE_THRESHOLD
        {
            return normalized;
        }
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let count = normalized.chars().count();
        let base = format!("[Pasted Content {count} chars]");
        let marker = if !current.contains(&base)
            && !self.entries.iter().any(|(existing, _)| existing == &base)
        {
            base
        } else {
            format!("[Pasted Content {count} chars #{}]", self.next_id)
        };
        self.entries.push((marker.clone(), normalized));
        marker
    }

    pub(crate) fn expand(&mut self, mut text: String) -> String {
        for (marker, original) in &self.entries {
            text = text.replace(marker, original);
        }
        self.entries.clear();
        text
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingToolApproval {
    pub(crate) approval_id: u64,
    pub(crate) tool_call_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExitState {
    #[default]
    Running,
    Requested,
    Finalize,
    Flush,
    FinalFrame,
    Done,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InfoState {
    pub(crate) status: String,
    pub(crate) provider: String,
    pub(crate) model_id: String,
    pub(crate) reasoning_effort: ReasoningLevel,
}

struct LiveTool {
    unit: HistoryUnitId,
    component: ComponentHandle<ConversationActivity>,
}

/// Application-owned conversation identities and semantic formatting state.
/// History owns the ordered presentation units themselves.
pub(crate) struct ConversationState {
    formatter: TuiFormatter,
    user_batch: Option<(HistoryUnitId, ComponentHandle<UserBatch>)>,
    working: Option<(HistoryUnitId, ComponentHandle<ConversationActivity>)>,
    tools: HashMap<String, LiveTool>,
    stream: Option<HistoryStreamHandle<AssistantStream>>,
    last_completed_tool: Option<String>,
    turn_started: bool,
}

impl Default for ConversationState {
    fn default() -> Self {
        Self {
            formatter: TuiFormatter::default(),
            user_batch: None,
            working: None,
            tools: HashMap::new(),
            stream: None,
            last_completed_tool: None,
            turn_started: false,
        }
    }
}

pub(crate) struct AppState {
    pub(crate) scene: Scene,
    pub(crate) components: ComponentRegistry,
    pub(crate) composer: ComponentHandle<TextInput>,
    pub(crate) steering: ComponentHandle<SteeringQueuePanel>,
    pub(crate) outputs: OutputRouter<AppAction>,
    pub(crate) paste_store: ComposerPasteStore,
    pub(crate) conversation: ConversationState,
    pub(crate) pending_tool_approval: Option<PendingToolApproval>,
    pub(crate) info: InfoState,
    pub(crate) exit_state: ExitState,
}

impl Default for AppState {
    fn default() -> Self {
        let mut components = ComponentRegistry::new();
        let composer = components.register(
            TextInput::new().multiline(true).border(
                crate::BorderSpec::plain()
                    .edges(crate::BorderEdges::TOP_BOTTOM)
                    .color(crate::ColorSpec::theme("input.border")),
            ),
        );
        let steering = components.register(SteeringQueuePanel::new());
        let mut outputs = OutputRouter::new();
        let submitted = components
            .with(composer, TextInput::submitted)
            .expect("composer registration disappeared");
        outputs
            .route(submitted, AppAction::SubmitTurn)
            .expect("composer output route must be unique");
        let mut state = Self {
            scene: Scene::with_history(History::new(), View::spacer(0)),
            components,
            composer,
            steering,
            outputs,
            paste_store: ComposerPasteStore::default(),
            conversation: ConversationState::default(),
            pending_tool_approval: None,
            info: InfoState::default(),
            exit_state: ExitState::default(),
        };
        state.rebuild_body();
        state
    }
}

impl AppState {
    fn history(&self) -> &History {
        self.scene
            .history()
            .expect("application Scene always has History")
    }

    fn history_mut(&mut self) -> &mut History {
        self.scene
            .history_mut()
            .expect("application Scene always has History")
    }

    pub(crate) fn rebuild_body(&mut self) {
        let composer = View::component(self.composer).fill_width();
        let steering = View::component(self.steering).fill_width();
        let footer = View::text(self.footer_text()).fill_width();
        self.scene.set_body(
            View::vertical(|column| {
                column.child(steering);
                column.content_max(MAX_COMPOSER_ROWS, composer);
                column.child(footer);
            })
            .fill_width()
            .fill_height(),
        );
    }

    fn footer_text(&self) -> String {
        [
            (!self.info.provider.is_empty()).then(|| self.info.provider.clone()),
            (!self.info.model_id.is_empty()).then(|| self.info.model_id.clone()),
            Some(format!("effort: {:?}", self.info.reasoning_effort)),
            (!self.info.status.is_empty()).then(|| self.info.status.clone()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
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
        self.rebuild_body();
    }

    pub(crate) fn cycle_reasoning_effort(&mut self) {
        self.info.reasoning_effort =
            ReasoningLevel::next_for(self.info.reasoning_effort, self.info.provider.as_str());
        self.rebuild_body();
    }

    pub(crate) fn request_exit(&mut self) {
        self.exit_state = ExitState::Requested;
    }

    pub(crate) fn clear_composer(&mut self) {
        let _ = self
            .components
            .with_mut(self.composer, |input| input.clear());
        self.paste_store.clear();
    }

    pub(crate) fn expand_submission(&mut self, text: String) -> String {
        self.paste_store.expand(text)
    }

    pub(crate) fn has_active_turn(&self) -> bool {
        self.conversation.turn_started
            || self.conversation.working.is_some()
            || self.conversation.stream.is_some()
            || !self.conversation.tools.is_empty()
            || self.pending_tool_approval.is_some()
    }

    pub(crate) fn turn_started(&mut self) -> Result<()> {
        self.conversation.turn_started = true;
        if self.conversation.user_batch.is_some() {
            self.start_working()?;
        }
        Ok(())
    }

    pub(crate) fn start_working(&mut self) -> Result<()> {
        if self.conversation.working.is_some() || self.conversation.stream.is_some() {
            return Ok(());
        }
        let component = self.components.register(ConversationActivity::working());
        let unit = self
            .history_mut()
            .push(View::component(component).fill_width())?;
        self.conversation.working = Some((unit, component));
        Ok(())
    }

    fn freeze_user_batch(&mut self) -> Result<()> {
        let Some((unit, component)) = self.conversation.user_batch.take() else {
            return Ok(());
        };
        let messages = self
            .components
            .with(component, |batch| batch.messages.clone())
            .ok_or_else(|| anyhow::anyhow!("user batch component disappeared"))?;
        let view = TuiFormatter::user_batch_view(&messages);
        self.history_mut().freeze(unit, view)?;
        self.components.remove(component);
        Ok(())
    }

    fn remove_working(&mut self) -> Result<()> {
        let Some((unit, component)) = self.conversation.working.take() else {
            return Ok(());
        };
        self.history_mut().discard_live(unit)?;
        self.components.remove(component);
        Ok(())
    }

    pub(crate) fn submit_user_message(&mut self, text: String) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let delivered_text = text.clone();
        if let Some((_, component)) = self.conversation.user_batch {
            self.components
                .with_mut(component, |batch| batch.push(text.clone()))
                .ok_or_else(|| anyhow::anyhow!("user batch component disappeared"))?;
        } else {
            let component = self.components.register(UserBatch::new(text));
            let unit = self
                .history_mut()
                .push(View::component(component).fill_width())?;
            self.conversation.user_batch = Some((unit, component));
        }
        let steering = self.steering;
        let _ = self
            .components
            .with_mut(steering, |panel| panel.delivered(&delivered_text));
        if self.conversation.turn_started {
            self.start_working()?;
        }
        self.rebuild_body();
        Ok(())
    }

    pub(crate) fn start_assistant_delta(
        &mut self,
        chunks: Vec<(SegmentKind, String)>,
    ) -> Result<()> {
        self.freeze_user_batch()?;
        let stream = if let Some((unit, component)) = self.conversation.working.take() {
            let stream = self
                .history_mut()
                .replace_live_with_stream(unit, AssistantStream::new())?;
            self.components.remove(component);
            stream
        } else if let Some(stream) = self.conversation.stream {
            stream
        } else {
            self.history_mut()
                .push_stream_with_boundary(AssistantStream::new(), FlowBoundary::Default)?
        };
        self.conversation.stream = Some(stream);
        self.push_stream_chunks(chunks)
    }

    fn push_stream_chunks(&mut self, chunks: Vec<(SegmentKind, String)>) -> Result<()> {
        let Some(stream) = self.conversation.stream else {
            return Ok(());
        };
        self.history_mut().update_stream(stream, |source| {
            for (kind, text) in chunks {
                source.push_delta(kind, &text);
            }
        })?;
        Ok(())
    }

    pub(crate) fn seal_stream(&mut self) -> Result<()> {
        let Some(stream) = self.conversation.stream.take() else {
            return Ok(());
        };
        self.history_mut().seal_stream(stream)?;
        Ok(())
    }

    pub(crate) fn finish_turn(&mut self) -> Result<()> {
        self.freeze_user_batch()?;
        self.seal_stream()?;
        self.conversation.turn_started = false;
        self.remove_working()?;
        Ok(())
    }

    pub(crate) fn finalize_for_exit(&mut self) -> Result<()> {
        self.freeze_user_batch()?;
        self.seal_stream()?;
        self.conversation.turn_started = false;
        self.remove_working()?;
        let tools = self.conversation.tools.keys().cloned().collect::<Vec<_>>();
        for tool_call_id in tools {
            self.finish_tool_call(tool_call_id, false)?;
        }
        Ok(())
    }

    pub(crate) fn fail_turn(&mut self, message: String) -> Result<()> {
        self.freeze_user_batch()?;
        self.seal_stream()?;
        self.conversation.turn_started = false;
        if self.conversation.working.is_some() {
            let (unit, component) = self.conversation.working.take().unwrap();
            let view = self
                .conversation
                .formatter
                .format(&TimelineItem::ErrorMessage { text: message });
            self.history_mut().freeze(unit, view)?;
            self.components.remove(component);
        } else if !message.is_empty() {
            let view = self
                .conversation
                .formatter
                .format(&TimelineItem::ErrorMessage { text: message });
            self.history_mut().push(view)?;
        }
        Ok(())
    }

    pub(crate) fn start_tool_call(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    ) -> Result<()> {
        self.freeze_user_batch()?;
        self.seal_stream()?;
        let status = ToolTimelineStatus::Running;
        let (unit, component) = if let Some((unit, component)) = self.conversation.working.take() {
            self.components
                .with_mut(component, |activity| {
                    activity.transition_to_tool(
                        tool_call_id.clone(),
                        tool_name.clone(),
                        arguments.clone(),
                        status,
                        None,
                    )
                })
                .ok_or_else(|| anyhow::anyhow!("working component disappeared"))?;
            (unit, component)
        } else {
            let component = self.components.register(ConversationActivity::tool(
                tool_call_id.clone(),
                tool_name.clone(),
                arguments.clone(),
                status,
                None,
            ));
            let unit = self
                .history_mut()
                .push(View::component(component).fill_width())?;
            (unit, component)
        };
        self.conversation
            .tools
            .insert(tool_call_id, LiveTool { unit, component });
        Ok(())
    }

    pub(crate) fn update_tool_call(
        &mut self,
        tool_call_id: String,
        update: ToolUpdatePresentation,
    ) -> Result<()> {
        let Some(tool) = self.conversation.tools.get(&tool_call_id) else {
            return Ok(());
        };
        let detail = format_tool_update(update);
        self.components
            .with_mut(tool.component, |activity| activity.update_tool(detail))
            .ok_or_else(|| anyhow::anyhow!("tool component disappeared"))?;
        Ok(())
    }

    pub(crate) fn request_tool_approval(
        &mut self,
        approval_id: u64,
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
    ) -> Result<()> {
        self.freeze_user_batch()?;
        if !self.conversation.tools.contains_key(&tool_call_id) {
            self.start_tool_call(tool_call_id.clone(), tool_name.clone(), arguments.clone())?;
        }
        let tool = self
            .conversation
            .tools
            .get(&tool_call_id)
            .ok_or_else(|| anyhow::anyhow!("tool component disappeared"))?;
        self.components
            .with_mut(tool.component, |activity| {
                activity.transition_to_tool(
                    tool_call_id.clone(),
                    tool_name,
                    arguments,
                    ToolTimelineStatus::PendingApproval,
                    Some(approval_id),
                )
            })
            .ok_or_else(|| anyhow::anyhow!("tool component disappeared"))?;
        let output = self
            .components
            .with(tool.component, ConversationActivity::approval_output)
            .ok_or_else(|| anyhow::anyhow!("tool component disappeared"))?;
        self.outputs
            .route(output, |decision: ApprovalDecision| {
                AppAction::ToolApproval(decision)
            })
            .map_err(|RouteConflict| anyhow::anyhow!("approval output route already exists"))?;
        self.pending_tool_approval = Some(PendingToolApproval {
            approval_id,
            tool_call_id,
        });
        Ok(())
    }

    pub(crate) fn resolve_tool_approval(
        &mut self,
        approval_id: u64,
        tool_call_id: String,
        approved: bool,
    ) -> Result<()> {
        if self
            .pending_tool_approval
            .as_ref()
            .is_some_and(|pending| pending.approval_id == approval_id)
        {
            self.pending_tool_approval = None;
        }
        if let Some(tool) = self.conversation.tools.get(&tool_call_id) {
            self.components.with_mut(tool.component, |activity| {
                activity.set_status(
                    if approved {
                        ToolTimelineStatus::Approved
                    } else {
                        ToolTimelineStatus::Rejected
                    },
                    None,
                )
            });
        }
        Ok(())
    }

    pub(crate) fn finish_tool_call(&mut self, tool_call_id: String, is_error: bool) -> Result<()> {
        let Some(tool) = self.conversation.tools.remove(&tool_call_id) else {
            return Ok(());
        };
        let item = self
            .components
            .with(tool.component, |activity| activity.final_item(is_error))
            .flatten()
            .ok_or_else(|| anyhow::anyhow!("tool component disappeared"))?;
        let view = self.conversation.formatter.format(&item);
        self.history_mut().freeze(tool.unit, view)?;
        self.components.remove(tool.component);
        self.conversation.last_completed_tool = Some(tool_call_id);
        Ok(())
    }

    pub(crate) fn push_tool_result(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        text: String,
        details: serde_json::Value,
        is_error: bool,
    ) -> Result<()> {
        let boundary = (self.conversation.last_completed_tool.as_ref() == Some(&tool_call_id))
            .then_some(FlowBoundary::AttachToPrevious)
            .unwrap_or(FlowBoundary::Default);
        let view = self
            .conversation
            .formatter
            .format(&TimelineItem::ToolResult {
                tool_call_id,
                tool_name,
                text,
                details,
                is_error,
                collapsed: false,
            });
        self.history_mut().push_with_boundary(view, boundary)?;
        Ok(())
    }

    pub(crate) fn enqueue_steer(&mut self, text: String) {
        let _ = self
            .components
            .with_mut(self.steering, |panel| panel.queued(text));
        self.rebuild_body();
    }

    pub(crate) fn apply_approval_action(
        &mut self,
        decision: ApprovalDecision,
        backend: &mut super::backend::BackendEventHandler,
    ) -> Result<()> {
        if decision.approved {
            backend.try_approve_tool_call(decision.approval_id)?;
        } else {
            backend
                .try_reject_tool_call(decision.approval_id, Some("Rejected by user".to_string()))?;
        }
        Ok(())
    }
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
    (!text.is_empty()).then(|| text.chars().take(1000).collect())
}

#[cfg(test)]
mod tests {
    use super::ComposerPasteStore;

    #[test]
    fn large_paste_markers_expand_and_reset() {
        let original = "x".repeat(1001);
        let mut store = ComposerPasteStore::default();
        let marker = store.display_text("", &original);
        assert!(marker.starts_with("[Pasted Content 1001 chars]"));
        assert_eq!(store.expand(marker), original);
        assert_eq!(store.expand("plain".to_string()), "plain");
    }

    #[test]
    fn colliding_large_paste_markers_are_unique() {
        let original = "x".repeat(1001);
        let mut store = ComposerPasteStore::default();
        let first = store.display_text("", &original);
        let second = store.display_text(&first, &original);
        assert_ne!(first, second);
        assert_eq!(
            store.expand(format!("{first} {second}")),
            format!("{original} {original}")
        );
    }
}
