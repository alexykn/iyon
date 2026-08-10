mod backend;
mod controller;
#[path = "components/conversation_activity.rs"]
mod final_components;
#[path = "components/steering_queue.rs"]
mod panel;
mod state;
mod stream_smoother;

mod theme;
pub(crate) mod tools;
pub(crate) mod transcript;

use anyhow::Result;
use iyon_core::ModelSelection;
use iyon_tui::{App, AppCx, History, HistoryLayout, Insets, View};

pub use backend::{FrontendEvent, ToolUpdatePresentation};
pub use controller::IyonAction;
pub use final_components::ApprovalDecision;
pub use state::IyonState;

use backend::{BackendCommands, CoreBridge};
use transcript::SegmentKind;

pub(crate) use theme::iyon_theme;

pub fn build_app(
    command_tx: iyon_core::CoreCommandSender,
    selection: ModelSelection,
) -> App<
    IyonState,
    IyonAction,
    anyhow::Error,
    impl FnOnce(&mut AppCx<'_, IyonAction>) -> Result<IyonState>,
    impl FnMut(&mut IyonState, IyonAction, &mut AppCx<'_, IyonAction>) -> Result<()>,
    impl Fn(&IyonState) -> View,
> {
    let init = move |cx: &mut AppCx<'_, IyonAction>| {
        IyonState::init(cx, BackendCommands::new(Some(command_tx)), &selection)
    };
    let update = update;
    let view = |state: &IyonState| state.view();
    let history = {
        let mut history = History::new();
        history.set_layout(HistoryLayout::new(Insets::new(0, 0, 1, 0), 1));
        history
    };
    App::new(init, update, view)
        .with_history(history)
        .with_theme(iyon_theme())
}

fn update(state: &mut IyonState, action: IyonAction, cx: &mut AppCx<'_, IyonAction>) -> Result<()> {
    match action {
        IyonAction::SubmitTurn(text) => {
            let text = state.paste_store.expand(text);
            state.clear_composer(cx);
            if matches!(
                state.backend.submit_turn(text)?,
                backend::SubmitTurnResult::NoBackendAttached
            ) {
                state.fail_turn(cx, "backend unavailable".to_string())?;
            }
        }
        IyonAction::ToolApproval(decision) => {
            if decision.approved {
                state.backend.approve(decision.approval_id)?;
            } else {
                state
                    .backend
                    .reject(decision.approval_id, Some("Rejected by user".to_string()))?;
            }
        }
        IyonAction::CtrlC => {
            let has_text = cx
                .with_component(state.composer, |input| !input.is_empty())
                .unwrap_or(false);
            if has_text {
                state.clear_composer(cx);
            } else if state.has_active_turn() {
                state.backend.cancel_active_turn()?;
            } else {
                state.prepare_goodbye(cx)?;
                cx.exit();
            }
        }
        IyonAction::Escape => {
            if state.has_active_turn() {
                state.backend.cancel_active_turn()?;
            }
        }
        IyonAction::RequestExit => {
            state.prepare_goodbye(cx)?;
            cx.exit();
        }
        IyonAction::InterruptActiveTurn => {
            if state.has_active_turn() {
                state.backend.cancel_active_turn()?;
            }
        }
        IyonAction::CycleReasoningEffort => {
            state.cycle_reasoning_effort();
            state.backend.cycle_reasoning_effort()?;
        }
        IyonAction::ComposerPaste(text) => {
            let current = cx
                .with_component(state.composer, |input| input.text().to_owned())
                .unwrap_or_default();
            let display = state.paste_store.display_text(&current, &text);
            cx.forward_paste(display);
        }
        IyonAction::StreamTick { generation } => {
            if generation != state.stream_pacer.generation {
                return Ok(());
            }
            state.stream_pacer.timer = None;
            if let Some(chunks) = state.stream_pacer.smoother.drain_ready(cx.now()) {
                state.start_assistant_delta(cx, chunks)?;
            }
            state.stream_pacer.schedule(cx);
        }
        IyonAction::Backend(event) => apply_backend_event(state, event, cx)?,
    }
    Ok(())
}

fn apply_backend_event(
    state: &mut IyonState,
    event: FrontendEvent,
    cx: &mut AppCx<'_, IyonAction>,
) -> Result<()> {
    match event {
        FrontendEvent::TurnStarted => state.turn_started(cx)?,
        FrontendEvent::SteerQueued { text } => state.enqueue_steer(cx, text),
        FrontendEvent::UserMessage { text } => state.submit_user_message(cx, text)?,
        FrontendEvent::AssistantDelta { text } => push_stream(state, cx, SegmentKind::Text, text)?,
        FrontendEvent::ThinkingDelta { text } => {
            push_stream(state, cx, SegmentKind::Thinking, text)?
        }
        FrontendEvent::TurnFinished => {
            if let Some(chunks) = state.stream_pacer.flush(cx) {
                state.start_assistant_delta(cx, chunks)?;
            }
            state.finish_turn(cx)?;
        }
        FrontendEvent::TurnFailed { message } => {
            if let Some(chunks) = state.stream_pacer.flush(cx) {
                state.start_assistant_delta(cx, chunks)?;
            }
            state.fail_turn(cx, message)?;
        }
        FrontendEvent::TurnCancelled => {
            state.stream_pacer.clear(cx);
            state.finish_turn(cx)?;
        }
        FrontendEvent::ToolCallStarted {
            tool_call_id,
            tool_name,
            arguments,
        } => {
            if let Some(chunks) = state.stream_pacer.flush(cx) {
                state.start_assistant_delta(cx, chunks)?;
            }
            state.start_tool_call(cx, tool_call_id, tool_name, arguments)?;
        }
        FrontendEvent::ToolCallUpdated {
            tool_call_id,
            update,
        } => state.update_tool_call(cx, tool_call_id, update)?,
        FrontendEvent::ToolCallFinished {
            tool_call_id,
            is_error,
        } => state.finish_tool_call(cx, tool_call_id, is_error)?,
        FrontendEvent::ToolApprovalRequested {
            approval_id,
            tool_call_id,
            tool_name,
            arguments,
        } => state.request_tool_approval(cx, approval_id, tool_call_id, tool_name, arguments)?,
        FrontendEvent::ToolApprovalResolved {
            approval_id,
            tool_call_id,
            approved,
            ..
        } => state.resolve_tool_approval(cx, approval_id, tool_call_id, approved)?,
        FrontendEvent::ToolResult {
            tool_call_id,
            tool_name,
            text,
            details,
            is_error,
        } => state.push_tool_result(cx, tool_call_id, tool_name, text, details, is_error)?,
        FrontendEvent::ConfigChanged {
            provider,
            model_id,
            reasoning_effort,
        } => state.apply_config(provider, model_id, reasoning_effort),
    }
    Ok(())
}

fn push_stream(
    state: &mut IyonState,
    cx: &mut AppCx<'_, IyonAction>,
    kind: SegmentKind,
    text: String,
) -> Result<()> {
    state.stream_pacer.smoother.push(kind, &text);
    state.stream_pacer.schedule(cx);
    Ok(())
}

pub async fn run_with_core(core: iyon_core::IyonCore, selection: ModelSelection) -> Result<()> {
    let (command_tx, event_rx) = core.split();
    let app = build_app(command_tx, selection);
    let handle = app.handle();
    let mut bridge = CoreBridge::spawn(event_rx, handle);
    let result = match app.run().await {
        Ok(()) => Ok(()),
        Err(iyon_tui::RunError::Application(error)) => Err(error),
        Err(iyon_tui::RunError::Runtime(error)) => Err(anyhow::Error::new(error)),
    };
    let bridge_result = bridge.shutdown().await;
    match result {
        Err(error) => Err(error),
        Ok(()) => bridge_result,
    }
}
