use std::collections::HashMap;

use anyhow::{Result, anyhow};
use iyon_api::ReasoningLevel;
use serde_json::Value;

use iyon_tui::{
    AppCx, BorderEdges, BorderSpec, ColorSpec, ComponentHandle, FlowBoundary, HistoryStreamHandle,
    HistoryUnitId, Key, KeyStroke, Modifiers, Output, RouteConflict, ScrollPane, TextInput, View,
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

#[derive(Clone, Copy)]
struct LiveTool {
    unit: HistoryUnitId,
    component: ComponentHandle<ConversationActivity>,
    output: ComponentHandle<ScrollPane>,
}

#[derive(Default)]
pub(crate) struct ConversationState {
    formatter: TuiFormatter,
    user_batch: Option<(HistoryUnitId, ComponentHandle<UserBatch>)>,
    working: Option<(HistoryUnitId, ComponentHandle<ConversationActivity>)>,
    tools: HashMap<String, LiveTool>,
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
        let steering = View::component(self.steering).fill_width();
        let footer = View::text(self.footer_text()).fill_width();
        View::vertical(|column| {
            column.child(steering);
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
            || !self.conversation.tools.is_empty()
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
        if self.conversation.working.is_some() || self.conversation.stream.is_some() {
            return Ok(());
        }
        let component = cx.register(ConversationActivity::working(
            self.conversation.formatter.clone(),
        ));
        let unit = cx
            .history_mut()
            .ok_or_else(|| anyhow!("history unavailable"))?
            .push(View::component(component).fill_width().fill_height())?;
        self.conversation.working = Some((unit, component));
        Ok(())
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
        let Some((unit, component)) = self.conversation.working.take() else {
            return Ok(());
        };
        cx.history_mut()
            .ok_or_else(|| anyhow!("history unavailable"))?
            .discard_live(unit)?;
        cx.remove_component(component);
        Ok(())
    }

    pub(crate) fn start_assistant_delta(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        chunks: Vec<(SegmentKind, String)>,
    ) -> Result<()> {
        self.freeze_user_batch(cx)?;
        let stream = if let Some((unit, component)) = self.conversation.working.take() {
            let stream = cx
                .history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .replace_live_with_stream(unit, AssistantStream::new())?;
            cx.remove_component(component);
            stream
        } else if let Some(stream) = self.conversation.stream {
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
        self.finalize_tools(cx)
    }

    pub(crate) fn fail_turn(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        message: String,
    ) -> Result<()> {
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        self.conversation.turn_started = false;
        let view = self
            .conversation
            .formatter
            .format(&TimelineItem::ErrorMessage {
                text: message.clone(),
            });
        if let Some((unit, component)) = self.conversation.working.take() {
            cx.history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .freeze(unit, view)?;
            cx.remove_component(component);
        } else if !message.is_empty() {
            cx.history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push(view)?;
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
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        let output = cx.register(ScrollPane::new(View::spacer(0)));
        let (unit, component) = if let Some((unit, component)) = self.conversation.working.take() {
            cx.with_component_mut(component, |activity| {
                activity.transition_to_tool(
                    id.clone(),
                    name.clone(),
                    args.clone(),
                    ToolTimelineStatus::Running,
                    None,
                    output,
                )
            })
            .ok_or_else(|| anyhow!("activity disappeared"))?;
            (unit, component)
        } else {
            let component = cx.register(ConversationActivity::tool(
                self.conversation.formatter.clone(),
                id.clone(),
                name.clone(),
                args.clone(),
                ToolTimelineStatus::Running,
                None,
                output,
            ));
            let unit = cx
                .history_mut()
                .ok_or_else(|| anyhow!("history unavailable"))?
                .push(View::component(component).fill_width().fill_height())?;
            (unit, component)
        };
        self.conversation.tools.insert(
            id,
            LiveTool {
                unit,
                component,
                output,
            },
        );
        Ok(())
    }

    pub(crate) fn update_tool_call(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: String,
        update: ToolUpdatePresentation,
    ) -> Result<()> {
        let Some(tool) = self.conversation.tools.get(&id).copied() else {
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
        if !self.conversation.tools.contains_key(&id) {
            self.start_tool_call(cx, id.clone(), name.clone(), args.clone())?;
        }
        let tool = self
            .conversation
            .tools
            .get(&id)
            .copied()
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
        if let Some(tool) = self.conversation.tools.get(&id).copied() {
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

    fn finalize_tools(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        let tools = self.conversation.tools.keys().cloned().collect::<Vec<_>>();
        for id in tools {
            self.finish_tool_call(cx, id.clone(), false)?;
            self.freeze_completed_tool(cx, &id, true)?;
        }
        Ok(())
    }

    fn freeze_completed_tool(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: &str,
        allow_missing: bool,
    ) -> Result<bool> {
        let Some(tool) = self.conversation.tools.get(id).copied() else {
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
        self.remove_approval_route(cx, id);
        cx.remove_component(tool.output);
        cx.remove_component(tool.component);
        self.conversation.tools.remove(id);
        self.conversation.last_completed_tool = Some(id.to_string());
        Ok(true)
    }

    pub(crate) fn finish_tool_call(
        &mut self,
        cx: &mut AppCx<'_, IyonAction>,
        id: String,
        is_error: bool,
    ) -> Result<()> {
        let Some(tool) = self.conversation.tools.get(&id).copied() else {
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
        if let Some(tool) = self.conversation.tools.get(&id).copied() {
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

    pub(crate) fn enqueue_steer(&mut self, cx: &mut AppCx<'_, IyonAction>, text: String) {
        let _ = cx.with_component_mut(self.steering, |panel| panel.queued(text));
    }

    pub(crate) fn prepare_goodbye(&mut self, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
        self.freeze_user_batch(cx)?;
        self.seal_stream(cx)?;
        self.conversation.turn_started = false;
        self.remove_working(cx)?;
        self.finalize_tools(cx)?;
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

    use super::{ConversationActivity, ToolTimelineStatus, TuiFormatter, tool_output_view};

    #[derive(Debug)]
    enum Action {
        Snapshot(String),
    }

    struct State {
        output: ComponentHandle<ScrollPane>,
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
