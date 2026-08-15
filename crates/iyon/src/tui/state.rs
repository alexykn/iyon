use std::collections::HashMap;

use anyhow::{Result, anyhow};
use iyon_api::ReasoningLevel;
use serde_json::Value;

use iyon_tui::{
    AppCx, BorderEdges, BorderSpec, ColorSpec, ComponentHandle, FlowBoundary, HistoryStreamHandle,
    HistoryUnitId, Insets, Key, KeyStroke, Modifiers, Output, RouteConflict, ScrollPane, TextInput,
    View,
};

use super::{
    backend::{BackendCommands, ToolUpdatePresentation},
    components::{ApprovalDecision, ConversationActivity, SteeringQueuePanel, UserBatch},
    controller::IyonAction,
    theme::{AGENT_EFFORT, effort_style_value},
    transcript::{AssistantStream, SegmentKind, TimelineItem, ToolTimelineStatus, TuiFormatter},
};

pub(crate) const MAX_COMPOSER_ROWS: u16 = 13;
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
    pub(crate) output: Output<ApprovalDecision>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InfoState {
    pub(crate) status: String,
    pub(crate) provider: String,
    pub(crate) model_id: String,
    pub(crate) reasoning_effort: ReasoningLevel,
}

#[derive(Clone)]
struct LiveTool {
    unit: HistoryUnitId,
    component: ComponentHandle<ConversationActivity>,
    output: ComponentHandle<ScrollPane>,
    draft_key: Option<super::backend::ToolDraftKey>,
    tool_call_id: Option<String>,
}

#[derive(Default)]
pub(crate) struct ConversationState {
    formatter: TuiFormatter,
    user_batch: Option<(HistoryUnitId, ComponentHandle<UserBatch>)>,
    working: Option<ComponentHandle<ConversationActivity>>,
    tools_by_unit: HashMap<HistoryUnitId, LiveTool>,
    draft_index: HashMap<super::backend::ToolDraftKey, HistoryUnitId>,
    tools: HashMap<String, HistoryUnitId>,
    stream: Option<HistoryStreamHandle<AssistantStream>>,
    last_completed_tool: Option<String>,
    turn_started: bool,
}

pub struct IyonState {
    pub(crate) backend: BackendCommands,
    pub(crate) composer: ComponentHandle<TextInput>,
    pub(crate) steering: ComponentHandle<SteeringQueuePanel>,
    pub(crate) paste_store: ComposerPasteStore,
    pub(crate) conversation: ConversationState,
    pub(crate) pending_tool_approval: Option<PendingToolApproval>,
    pub(crate) info: InfoState,
    pub(crate) body_visible: bool,
}

impl IyonState {
    pub(crate) fn init(
        cx: &mut AppCx<'_, IyonAction>,
        backend: BackendCommands,
        selection: &iyon_core::ModelSelection,
    ) -> Result<Self> {
        let composer = cx.register(
            TextInput::new().multiline(true).border(
                BorderSpec::plain()
                    .edges(BorderEdges::TOP_BOTTOM)
                    .color(ColorSpec::theme("input.border")),
            ),
        );
        let steering = cx.register(SteeringQueuePanel::new());
        let submitted = cx
            .with_component(composer, TextInput::submitted)
            .ok_or_else(|| anyhow!("composer disappeared"))?;
        cx.route(submitted, IyonAction::SubmitTurn)
            .map_err(|RouteConflict| anyhow!("composer output route must be unique"))?;
        cx.intercept_paste(composer, IyonAction::ComposerPaste);
        cx.bind_key(
            KeyStroke::with_modifiers(Key::Char('c'), Modifiers::CONTROL),
            || IyonAction::CtrlC,
        );
        cx.bind_key(iyon_tui::KeyStroke::new(iyon_tui::Key::Escape), || {
            IyonAction::Escape
        });
        cx.bind_key(
            KeyStroke::with_modifiers(Key::Tab, Modifiers::SHIFT),
            || IyonAction::CycleReasoningEffort,
        );
        Ok(Self {
            backend,
            composer,
            steering,
            paste_store: ComposerPasteStore::default(),
            conversation: ConversationState::default(),
            pending_tool_approval: None,
            info: InfoState {
                provider: selection.provider.clone(),
                model_id: selection.model_id.clone(),
                ..InfoState::default()
            },
            body_visible: true,
        })
    }

    pub(crate) fn view(&self) -> View {
        if !self.body_visible {
            return View::spacer(0);
        }
        let composer = View::component(self.composer)
            .style_state(
                AGENT_EFFORT.clone(),
                effort_style_value(self.info.reasoning_effort),
            )
            .fill_width();
        let footer = View::text(self.footer_text()).fill_width();
        View::vertical(|column| {
            if let Some(working) = self.conversation.working {
                column.child(
                    View::component(working)
                        .fill_width()
                        .padding(Insets::new(0, 0, 1, 0)),
                );
            }
            column.content_max(MAX_COMPOSER_ROWS, composer);
            column.child(footer);
        })
        .fill_width()
        .fill_height()
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
        effort: ReasoningLevel,
    ) {
        self.info.provider = provider;
        self.info.model_id = model_id;
        self.info.reasoning_effort = effort;
    }

    pub(crate) fn cycle_reasoning_effort(&mut self) {
        self.info.reasoning_effort =
            ReasoningLevel::next_for(self.info.reasoning_effort, self.info.provider.as_str());
    }

    pub(crate) fn has_active_turn(&self) -> bool {
        self.conversation.turn_started
            || self.conversation.working.is_some()
            || self.conversation.stream.is_some()
            || !self.conversation.tools_by_unit.is_empty()
            || self.pending_tool_approval.is_some()
    }

    pub(crate) fn clear_composer(&mut self, cx: &mut AppCx<'_, IyonAction>) {
        let _ = cx.with_component_mut(self.composer, TextInput::clear);
        self.paste_store.clear();
    }

    pub(crate) fn submit_user_message(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        text: String,
    ) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        // A steered user message is emitted at the boundary between two model
        // responses. The core can emit it before the previous turn's final
        // TurnFinished reaches the UI, so close the assistant stream before
        // appending the new History unit.
        self.seal_stream(cx)?;
        if let Some((_, component)) = self.conversation.user_batch {
            cx.with_component_mut(component, |batch| batch.push(text.clone()))
                .ok_or_else(|| anyhow!("user batch disappeared"))?;
        } else {
            let component = cx.register(UserBatch::new(text.clone()));
            let unit = cx
                .history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push(View::component(component).fill_width())?;
            self.conversation.user_batch = Some((unit, component));
        }
        let _ = cx.with_component_mut(self.steering, |panel| panel.delivered(&text));
        if let Some(working) = self.conversation.working {
            let _ = cx.with_component_mut(working, |activity| activity.delivered(&text));
        }
        if self.conversation.turn_started {
            self.start_working(cx)?;
        }
        Ok(())
    }

    pub(crate) fn turn_started(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        self.conversation.turn_started = true;
        if self.conversation.user_batch.is_some() {
            self.start_working(cx)?;
        }
        Ok(())
    }

    fn start_working(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        if self.conversation.working.is_some() {
            return Ok(());
        }
        let pending_steers = cx
            .with_component(self.steering, SteeringQueuePanel::pending)
            .unwrap_or_default();
        if self.conversation.stream.is_some() && pending_steers.is_empty() {
            return Ok(());
        }
        self.conversation.working = Some(cx.register(ConversationActivity::working(
            self.conversation.formatter.clone(),
            pending_steers,
        )));
        Ok(())
    }

    fn working_has_queue(&self, cx: &AppCx<'_, IyonAction>) -> bool {
        self.conversation.working.is_some_and(|working| {
            cx.with_component(working, ConversationActivity::has_queue)
                .unwrap_or(false)
        })
    }

    fn freeze_user_batch(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        let Some((unit, component)) = self.conversation.user_batch.take() else {
            return Ok(());
        };
        let messages = cx
            .with_component(component, |batch| batch.messages.clone())
            .ok_or_else(|| anyhow!("user batch disappeared"))?;
        let view = TuiFormatter::user_batch_view(&messages);
        cx.history_mut()
            .ok_or_else(|| anyhow!("history unavailable"))?
            .freeze(unit, view)?;
        cx.remove_component(component);
        Ok(())
    }

    fn remove_working(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        let Some(working) = self.conversation.working.take() else {
            return Ok(());
        };
        cx.remove_component(working);
        Ok(())
    }

    pub(crate) fn start_assistant_delta(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        chunks: Vec<(SegmentKind, String)>,
    ) -> Result<()> {
        self.freeze_user_batch(cx)?;
        if !self.working_has_queue(cx) {
            self.remove_working(cx)?;
        }
        let stream = if let Some(stream) = self.conversation.stream {
            stream
        } else {
            cx.history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push_stream_with_boundary(AssistantStream::new(), FlowBoundary::Default)?
        };
        self.conversation.stream = Some(stream);
        cx.history_mut()
            .ok_or_else(|| anyhow!("history unavailable"))?
            .update_stream(stream, |source| {
                for (kind, text) in chunks {
                    source.push_delta_paced(kind, &text);
                }
            })?;
        Ok(())
    }

    pub(crate) fn seal_stream(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        if let Some(stream) = self.conversation.stream.take() {
            cx.history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .seal_stream(stream)?;
        }
        Ok(())
    }

    pub(crate) fn finish_turn(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        self.conversation.turn_started = false;
        self.remove_working(cx)?;
        self.terminalize_live_tools(cx, false)
    }

    pub(crate) fn cancel_turn(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        self.conversation.turn_started = false;
        self.remove_working(cx)?;
        self.terminalize_live_tools(cx, true)
    }

    pub(crate) fn fail_turn(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        message: String,
    ) -> Result<()> {
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        self.conversation.turn_started = false;
        self.terminalize_live_tools(cx, false)?;
        let view = self
            .conversation
            .formatter
            .format(&TimelineItem::ErrorMessage {
                text: message.clone(),
            });
        self.remove_working(cx)?;
        if !message.is_empty() {
            cx.history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push(view)?;
        }
        Ok(())
    }

    fn live_tool_for_id(&self, id: &str) -> Option<LiveTool> {
        self.conversation
            .tools
            .get(id)
            .and_then(|unit| self.conversation.tools_by_unit.get(unit))
            .cloned()
    }

    fn live_tool_for_draft(&self, key: super::backend::ToolDraftKey) -> Option<LiveTool> {
        self.conversation
            .draft_index
            .get(&key)
            .and_then(|unit| self.conversation.tools_by_unit.get(unit))
            .cloned()
    }

    fn materialize_tool(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        draft_key: Option<super::backend::ToolDraftKey>,
        id: String,
        name: String,
        args: Value,
        status: ToolTimelineStatus,
    ) -> Result<LiveTool> {
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        let output = cx.register(ScrollPane::new(View::spacer(0)));
        let keep_working = self.working_has_queue(cx);
        let (unit, component) = if keep_working {
            let component = cx.register(ConversationActivity::tool(
                self.conversation.formatter.clone(),
                id.clone(),
                name.clone(),
                args.clone(),
                status,
                None,
                output,
            ));
            let unit = cx
                .history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push(View::component(component).fill_width().fill_height())?;
            (unit, component)
        } else if let Some(working) = self.conversation.working.take() {
            cx.with_component_mut(working, |activity| {
                activity.transition_to_tool(
                    id.clone(),
                    name.clone(),
                    args.clone(),
                    status,
                    None,
                    output,
                )
            })
            .ok_or_else(|| anyhow!("activity disappeared"))?;
            let unit = cx
                .history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push(View::component(working).fill_width().fill_height())?;
            (unit, working)
        } else {
            let component = cx.register(ConversationActivity::tool(
                self.conversation.formatter.clone(),
                id.clone(),
                name.clone(),
                args.clone(),
                status,
                None,
                output,
            ));
            let unit = cx
                .history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push(View::component(component).fill_width().fill_height())?;
            (unit, component)
        };
        let live = LiveTool {
            unit,
            component,
            output,
            draft_key,
            tool_call_id: (!id.is_empty()).then_some(id),
        };
        self.conversation.tools_by_unit.insert(unit, live.clone());
        if let Some(key) = draft_key {
            self.conversation.draft_index.insert(key, unit);
        }
        if self.conversation.turn_started && self.conversation.working.is_none() {
            self.start_working(cx)?;
        }
        Ok(live)
    }

    pub(crate) fn start_tool_draft(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        key: super::backend::ToolDraftKey,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
    ) -> Result<()> {
        if let Some(tool) = self.live_tool_for_draft(key) {
            cx.with_component_mut(tool.component, |activity| {
                activity.update_tool_identity(tool_call_id, tool_name)
            })
            .ok_or_else(|| anyhow!("activity disappeared"))?;
            return Ok(());
        }
        self.materialize_tool(
            cx,
            Some(key),
            tool_call_id.unwrap_or_default(),
            tool_name.unwrap_or_default(),
            serde_json::json!({}),
            ToolTimelineStatus::Preparing,
        )?;
        Ok(())
    }

    pub(crate) fn append_tool_draft_arguments(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        key: super::backend::ToolDraftKey,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
        delta: String,
    ) -> Result<()> {
        self.start_tool_draft(cx, key, tool_call_id, tool_name)?;
        let tool = self
            .live_tool_for_draft(key)
            .ok_or_else(|| anyhow!("tool draft disappeared"))?;
        cx.with_component_mut(tool.component, |activity| {
            activity.append_argument_preview(&delta)
        })
        .ok_or_else(|| anyhow!("activity disappeared"))?;
        Ok(())
    }

    pub(crate) fn prepare_tool_draft(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        key: super::backend::ToolDraftKey,
        id: String,
        name: String,
        args: Value,
    ) -> Result<()> {
        let tool = if let Some(tool) = self.live_tool_for_draft(key) {
            tool
        } else {
            self.materialize_tool(
                cx,
                Some(key),
                id.clone(),
                name.clone(),
                args.clone(),
                ToolTimelineStatus::Prepared,
            )?
        };
        if let Some(previous_id) = tool.tool_call_id.as_deref() {
            if self
                .conversation
                .tools
                .get(previous_id)
                .is_some_and(|unit| *unit == tool.unit)
            {
                self.conversation.tools.remove(previous_id);
            }
        }
        cx.with_component_mut(tool.component, |activity| {
            activity.prepare_tool(id.clone(), name, args)
        })
        .ok_or_else(|| anyhow!("activity disappeared"))?;
        if let Some(live) = self.conversation.tools_by_unit.get_mut(&tool.unit) {
            live.tool_call_id = (!id.is_empty()).then_some(id.clone());
        }
        if !id.is_empty() {
            self.conversation.tools.insert(id, tool.unit);
        }
        Ok(())
    }

    pub(crate) fn start_tool_call(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: String,
        name: String,
        args: Value,
    ) -> Result<()> {
        if let Some(tool) = self.live_tool_for_id(&id) {
            self.remove_approval_route(cx, &id);
            cx.with_component_mut(tool.component, ConversationActivity::start_execution)
                .ok_or_else(|| anyhow!("activity disappeared"))?;
            return Ok(());
        }
        let tool = self.materialize_tool(
            cx,
            None,
            id.clone(),
            name,
            args,
            ToolTimelineStatus::Running,
        )?;
        self.conversation.tools.insert(id, tool.unit);
        Ok(())
    }

    pub(crate) fn update_tool_call(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: String,
        update: ToolUpdatePresentation,
    ) -> Result<()> {
        let Some(tool) = self.live_tool_for_id(&id) else {
            return Ok(());
        };
        cx.with_component_mut(tool.output, |output| {
            output.set_content(tool_output_view(format_tool_update(update)));
        })
        .ok_or_else(|| anyhow!("tool output disappeared"))?;
        Ok(())
    }

    pub(crate) fn request_tool_approval(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        approval_id: u64,
        id: String,
        name: String,
        args: Value,
    ) -> Result<()> {
        self.freeze_user_batch(cx)?;
        if self.live_tool_for_id(&id).is_none() {
            self.start_tool_call(cx, id.clone(), name.clone(), args.clone())?;
        }
        let tool = self
            .live_tool_for_id(&id)
            .ok_or_else(|| anyhow!("tool disappeared"))?;
        cx.with_component_mut(tool.component, |activity| {
            activity.transition_to_tool(
                id.clone(),
                name,
                args,
                ToolTimelineStatus::PendingApproval,
                Some(approval_id),
                tool.output,
            )
        })
        .ok_or_else(|| anyhow!("activity disappeared"))?;
        let output = cx
            .with_component(tool.component, ConversationActivity::approval_output)
            .ok_or_else(|| anyhow!("activity disappeared"))?;
        cx.route(output, IyonAction::ToolApproval)
            .map_err(|RouteConflict| anyhow!("approval route conflict"))?;
        self.pending_tool_approval = Some(PendingToolApproval {
            approval_id,
            tool_call_id: id,
            output,
        });
        Ok(())
    }

    fn remove_approval_route(&mut self, cx: &mut AppCx<'_, IyonAction>, id: &str) {
        let Some(pending) = self.pending_tool_approval.take() else {
            return;
        };
        if pending.tool_call_id == id {
            cx.remove_route(pending.output);
        } else {
            self.pending_tool_approval = Some(pending);
        }
    }

    pub(crate) fn resolve_tool_approval(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        approval_id: u64,
        id: String,
        approved: bool,
    ) -> Result<()> {
        if self
            .pending_tool_approval
            .as_ref()
            .is_some_and(|pending| pending.approval_id == approval_id)
        {
            self.remove_approval_route(cx, &id);
        }
        if let Some(tool) = self.live_tool_for_id(&id) {
            cx.with_component_mut(tool.component, |activity| {
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

    fn terminalize_live_tools(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        cancelled: bool,
    ) -> Result<()> {
        let units = self
            .conversation
            .tools_by_unit
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for unit in units {
            let Some(tool) = self.conversation.tools_by_unit.get(&unit).cloned() else {
                continue;
            };
            cx.with_component_mut(tool.component, |activity| {
                if cancelled {
                    activity.cancel();
                } else {
                    activity.fail();
                }
            })
            .ok_or_else(|| anyhow!("activity disappeared"))?;
            self.freeze_tool(cx, unit, true)?;
        }
        Ok(())
    }

    fn freeze_tool(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        unit: HistoryUnitId,
        allow_missing: bool,
    ) -> Result<bool> {
        let Some(tool) = self.conversation.tools_by_unit.get(&unit).cloned() else {
            return Ok(false);
        };
        let ready = cx
            .with_component(tool.component, |activity| {
                activity.is_finished() && (allow_missing || activity.has_result())
            })
            .ok_or_else(|| anyhow!("activity disappeared"))?;
        if !ready {
            return Ok(false);
        }
        let view = cx
            .with_component(tool.component, ConversationActivity::final_view)
            .flatten()
            .ok_or_else(|| anyhow!("activity disappeared"))?;
        cx.history_mut()
            .ok_or_else(|| anyhow!("history unavailable"))?
            .freeze(tool.unit, view)?;
        if let Some(id) = tool.tool_call_id.as_deref() {
            let is_final_id = self
                .conversation
                .tools
                .get(id)
                .is_some_and(|mapped| *mapped == unit);
            if is_final_id {
                self.remove_approval_route(cx, id);
                self.conversation.tools.remove(id);
                self.conversation.last_completed_tool = Some(id.to_string());
            }
        }
        if let Some(key) = tool.draft_key {
            if self
                .conversation
                .draft_index
                .get(&key)
                .is_some_and(|mapped| *mapped == unit)
            {
                self.conversation.draft_index.remove(&key);
            }
        }
        cx.remove_component(tool.output);
        cx.remove_component(tool.component);
        self.conversation.tools_by_unit.remove(&unit);
        Ok(true)
    }

    fn freeze_completed_tool(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: &str,
        allow_missing: bool,
    ) -> Result<bool> {
        let Some(unit) = self.conversation.tools.get(id).copied() else {
            return Ok(false);
        };
        self.freeze_tool(cx, unit, allow_missing)
    }

    pub(crate) fn finish_tool_call(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: String,
        is_error: bool,
    ) -> Result<()> {
        let Some(tool) = self.live_tool_for_id(&id) else {
            return Ok(());
        };
        cx.with_component_mut(tool.component, |activity| activity.complete(is_error))
            .ok_or_else(|| anyhow!("activity disappeared"))?;
        self.freeze_completed_tool(cx, &id, false)?;
        Ok(())
    }

    pub(crate) fn push_tool_result(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: String,
        name: String,
        text: String,
        details: Value,
        is_error: bool,
    ) -> Result<()> {
        if self.conversation.last_completed_tool.as_ref() == Some(&id) {
            return Ok(());
        }
        if let Some(tool) = self.live_tool_for_id(&id) {
            cx.with_component_mut(tool.output, |output| {
                output.set_content(tool_output_view(Some(text.clone())));
            })
            .ok_or_else(|| anyhow!("tool output disappeared"))?;
            cx.with_component_mut(tool.component, |activity| {
                activity.set_result(text, details, is_error);
                activity.complete(is_error);
            })
            .ok_or_else(|| anyhow!("activity disappeared"))?;
            self.freeze_completed_tool(cx, &id, true)?;
            return Ok(());
        }
        let boundary = if self.conversation.last_completed_tool.as_ref() == Some(&id) {
            FlowBoundary::AttachToPrevious
        } else {
            FlowBoundary::Default
        };
        let view = self
            .conversation
            .formatter
            .format(&TimelineItem::ToolResult {
                tool_call_id: id,
                tool_name: name,
                text,
                details,
                is_error,
                collapsed: true,
            });
        cx.history_mut()
            .ok_or_else(|| anyhow!("history unavailable"))?
            .push_with_boundary(view, boundary)?;
        Ok(())
    }

    pub(crate) fn enqueue_steer(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        text: String,
    ) -> Result<()> {
        let _ = cx.with_component_mut(self.steering, |panel| panel.queued(text.clone()));
        if let Some(working) = self.conversation.working {
            let _ = cx.with_component_mut(working, |activity| activity.queued(text));
        } else if self.conversation.turn_started {
            self.start_working(cx)?;
        }
        Ok(())
    }

    pub(crate) fn prepare_goodbye(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        self.conversation.turn_started = false;
        self.remove_working(cx)?;
        self.terminalize_live_tools(cx, true)?;
        cx.history_mut()
            .ok_or_else(|| anyhow!("history unavailable"))?
            .push(View::text("Goodbye.").fill_width())?;
        self.body_visible = false;
        Ok(())
    }
}

fn tool_output_view(text: Option<String>) -> View {
    let Some(text) = text else {
        return View::spacer(0);
    };
    View::hanging(
        View::text("  ").no_wrap(),
        View::text("  ").no_wrap(),
        View::text(text)
            .foreground(ColorSpec::theme("text.muted"))
            .fill_width(),
    )
    .fill_width()
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
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use iyon_tui::{App, AppCx, ComponentHandle, History, ScrollPane, View, testing};

    use super::super::components::conversation_activity::ActivityState;
    use super::{
        BackendCommands, ConversationActivity, IyonAction, IyonState, ToolTimelineStatus,
        TuiFormatter, tool_output_view,
    };

    #[derive(Debug)]
    enum Action {
        Snapshot(String),
    }

    struct State {
        output: ComponentHandle<ScrollPane>,
    }

    #[test]
    fn draft_lifecycle_reuses_one_live_tool_and_keeps_siblings_separate() {
        let app = App::new(
            |cx| {
                let selection = iyon_core::ModelSelection {
                    provider: "test".into(),
                    model_id: "test".into(),
                };
                let mut state = IyonState::init(cx, BackendCommands::new(None), &selection)?;
                state
                    .start_assistant_delta(cx, vec![(super::SegmentKind::Text, "before".into())])?;
                let key = super::super::backend::ToolDraftKey {
                    message_id: 7,
                    content_index: 0,
                };
                state.start_tool_draft(cx, key, None, Some("bash".into()))?;
                let first = state.conversation.draft_index[&key];
                let first_live = state.conversation.tools_by_unit[&first].clone();
                assert!(state.conversation.stream.is_none());
                assert!(matches!(
                    cx.with_component(first_live.component, |activity| matches!(
                        &activity.state,
                        ActivityState::Tool {
                            status: ToolTimelineStatus::Preparing,
                            ..
                        }
                    )),
                    Some(true)
                ));

                state.append_tool_draft_arguments(
                    cx,
                    key,
                    None,
                    None,
                    "{\"command\":\"pwd\"}".into(),
                )?;
                let after_arguments = state.conversation.tools_by_unit[&first].clone();
                assert_eq!(first_live.unit, after_arguments.unit);
                assert_eq!(first_live.component, after_arguments.component);
                assert_eq!(state.conversation.tools_by_unit.len(), 1);

                state.prepare_tool_draft(
                    cx,
                    key,
                    "call-1".into(),
                    "bash".into(),
                    serde_json::json!({"command": "pwd"}),
                )?;
                assert_eq!(state.conversation.tools["call-1"], first);
                assert!(matches!(
                    cx.with_component(first_live.component, |activity| matches!(
                        &activity.state,
                        ActivityState::Tool {
                            status: ToolTimelineStatus::Prepared,
                            ..
                        }
                    )),
                    Some(true)
                ));
                state.start_tool_call(cx, "call-1".into(), "bash".into(), serde_json::json!({}))?;
                let after_start = state.conversation.tools_by_unit[&first].clone();
                assert_eq!(after_start.unit, first_live.unit);
                assert_eq!(after_start.component, first_live.component);
                assert!(matches!(
                    cx.with_component(first_live.component, |activity| matches!(
                        &activity.state,
                        ActivityState::Tool {
                            status: ToolTimelineStatus::Running,
                            ..
                        }
                    )),
                    Some(true)
                ));

                let second_key = super::super::backend::ToolDraftKey {
                    message_id: 7,
                    content_index: 1,
                };
                state.start_tool_draft(cx, second_key, None, Some("read".into()))?;
                state.prepare_tool_draft(
                    cx,
                    second_key,
                    "call-2".into(),
                    "read".into(),
                    serde_json::json!({"path": "README.md"}),
                )?;
                let second = state.conversation.tools["call-2"];
                assert_ne!(first, second);
                state.start_tool_call(cx, "call-1".into(), "bash".into(), serde_json::json!({}))?;
                assert!(matches!(
                    cx.with_component(
                        state.conversation.tools_by_unit[&second].component,
                        |activity| matches!(
                            &activity.state,
                            ActivityState::Tool {
                                status: ToolTimelineStatus::Prepared,
                                ..
                            }
                        )
                    ),
                    Some(true)
                ));

                let fallback_id = "legacy".to_string();
                state.start_tool_call(
                    cx,
                    fallback_id.clone(),
                    "bash".into(),
                    serde_json::json!({"command": "true"}),
                )?;
                let fallback = state.conversation.tools[&fallback_id];
                state.start_tool_call(
                    cx,
                    fallback_id.clone(),
                    "bash".into(),
                    serde_json::json!({}),
                )?;
                assert_eq!(state.conversation.tools[&fallback_id], fallback);
                Ok::<IyonState, anyhow::Error>(state)
            },
            |_state: &mut IyonState, _action: IyonAction, _cx| Ok::<(), anyhow::Error>(()),
            |state: &IyonState| state.view(),
        )
        .with_history(History::new());
        testing::start(app, 80, 20).unwrap();
    }

    #[test]
    fn cancellation_and_failure_freeze_live_drafts_without_success() {
        let app = App::new(
            |cx| {
                let selection = iyon_core::ModelSelection {
                    provider: "test".into(),
                    model_id: "test".into(),
                };
                let mut state = IyonState::init(cx, BackendCommands::new(None), &selection)?;
                let key = super::super::backend::ToolDraftKey {
                    message_id: 9,
                    content_index: 0,
                };
                state.start_tool_draft(cx, key, None, Some("bash".into()))?;
                state.cancel_turn(cx)?;
                assert!(state.conversation.tools_by_unit.is_empty());
                assert!(state.conversation.draft_index.is_empty());
                assert!(state.conversation.tools.is_empty());

                let failed_key = super::super::backend::ToolDraftKey {
                    message_id: 10,
                    content_index: 0,
                };
                state.start_tool_draft(cx, failed_key, None, Some("read".into()))?;
                state.fail_turn(cx, "turn failed".into())?;
                assert!(state.conversation.tools_by_unit.is_empty());
                Ok::<IyonState, anyhow::Error>(state)
            },
            |_state: &mut IyonState, _action: IyonAction, _cx| Ok::<(), anyhow::Error>(()),
            |state: &IyonState| state.view(),
        )
        .with_history(History::new());
        let harness = testing::start(app, 80, 20).unwrap();
        let lines = harness.screen_lines();
        assert!(lines.iter().any(|line| line.contains("cancelled")));
        assert!(lines.iter().any(|line| line.contains("failed")));
        assert!(lines.iter().any(|line| line.contains("turn failed")));
    }

    fn snapshot(count: usize, prefix: &str) -> String {
        (1..=count)
            .map(|row| format!("{prefix} {row}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn live_tool_snapshots_keep_the_mounted_scroll_pane_visible() {
        let app = App::new(
            |cx: &mut AppCx<'_, Action>| {
                let output = cx.register(ScrollPane::new(View::spacer(0)));
                let activity = cx.register(ConversationActivity::tool(
                    TuiFormatter::default(),
                    "call".into(),
                    "bash".into(),
                    serde_json::json!({"command": "printf"}),
                    ToolTimelineStatus::Running,
                    None,
                    output,
                ));
                cx.history_mut()
                    .ok_or_else(|| anyhow::anyhow!("history unavailable"))?
                    .push(View::component(activity).fill_width().fill_height())?;
                Ok::<State, anyhow::Error>(State { output })
            },
            |state: &mut State, Action::Snapshot(text), cx| {
                // The update uses the same full rolling snapshot path as the
                // production tool action; it never appends individual chunks.
                cx.with_component_mut(state.output, |output| {
                    output.set_content(tool_output_view(Some(text)))
                })
                .ok_or_else(|| anyhow::anyhow!("tool output disappeared"))?;
                Ok::<(), anyhow::Error>(())
            },
            |_state: &State| {
                View::vertical(|column| {
                    column.child(View::text("composer\nfooter"));
                })
            },
        )
        .with_history(History::new());
        let mut harness = testing::start(app, 40, 24).expect("start harness");

        let mut apply = |text: String| {
            harness
                .handle()
                .send(Action::Snapshot(text))
                .expect("snapshot action");
            harness.step().expect("snapshot step");
            harness.screen_lines()
        };

        let one = apply(snapshot(1, "line"));
        assert_eq!(one.iter().filter(|line| line.contains("line 1")).count(), 1);

        let five = apply(snapshot(5, "line"));
        assert_eq!(
            five.iter()
                .filter(|line| line.starts_with("  line "))
                .count(),
            5
        );

        let twenty = apply(snapshot(20, "line"));
        let visible_twenty = twenty
            .iter()
            .filter(|line| line.starts_with("  line "))
            .count();
        assert_eq!(visible_twenty, 16);
        assert!(twenty.iter().any(|line| line.contains("line 20")));

        let rolling = apply(snapshot(20, "next"));
        let visible_rolling = rolling
            .iter()
            .filter(|line| line.starts_with("  next "))
            .count();
        assert_eq!(visible_rolling, 16);
        assert!(rolling.iter().any(|line| line.contains("next 20")));
    }
}
