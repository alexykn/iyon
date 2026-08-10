use std::time::{Duration, Instant};

use crate::{
    runtime::{
        AppAction, BackendEventHandler, FrontendEvent, SceneHost, StreamSmoother,
        final_state::{AppState, ExitState},
    },
    terminal::crossterm::key::key_stroke,
    terminal::{InlineHistorySink, InlineTerminal},
};
use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

const IDLE_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const BACKEND_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) struct App {
    backend_handler: BackendEventHandler,
    scene_host: SceneHost,
    stream_smoother: StreamSmoother,
    needs_redraw: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            backend_handler: BackendEventHandler::default(),
            scene_host: SceneHost::default(),
            stream_smoother: StreamSmoother::default(),
            needs_redraw: true,
        }
    }
}

impl App {
    pub(super) fn with_backend_handler(backend_handler: BackendEventHandler) -> Self {
        Self {
            backend_handler,
            ..Self::default()
        }
    }

    pub(super) fn run(&mut self, terminal: &mut InlineTerminal) -> Result<()> {
        let mut state = AppState::default();
        let initial_events = self.backend_handler.drain_events()?;
        self.apply_backend_events(initial_events, &mut state)?;

        loop {
            match state.exit_state {
                ExitState::Running => self.step_running(terminal, &mut state)?,
                ExitState::Requested => {
                    state.exit_state = ExitState::Finalize;
                    self.needs_redraw = true;
                }
                ExitState::Finalize => {
                    self.flush_stream(&mut state)?;
                    state.finalize_for_exit()?;
                    state.exit_state = ExitState::Flush;
                    self.needs_redraw = true;
                }
                ExitState::Flush => {
                    self.render(terminal, &mut state)?;
                    state.exit_state = ExitState::FinalFrame;
                }
                ExitState::FinalFrame => {
                    self.render_goodbye(terminal, &mut state)?;
                    state.exit_state = ExitState::Done;
                }
                ExitState::Done => break,
            }
        }
        Ok(())
    }

    fn step_running(&mut self, terminal: &mut InlineTerminal, state: &mut AppState) -> Result<()> {
        if self.needs_redraw {
            self.render(terminal, state)?;
            return Ok(());
        }

        let timeout = self.next_wait_timeout(state);
        if event::poll(timeout)? {
            self.handle_event(event::read()?, state)?;
            while event::poll(Duration::ZERO)? {
                self.handle_event(event::read()?, state)?;
            }
        }

        let backend_events = self.backend_handler.drain_events()?;
        self.apply_backend_events(backend_events, state)?;
        self.flush_stream_if_ready(state)?;

        if self
            .scene_host
            .tick_due(Instant::now(), &mut state.components)
        {
            self.apply_output_actions(state)?;
            self.needs_redraw = true;
        }
        if state.exit_state == ExitState::Running && self.needs_redraw {
            self.render(terminal, state)?;
        }
        Ok(())
    }

    fn handle_event(&mut self, event: Event, state: &mut AppState) -> Result<()> {
        match event {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if let Some(stroke) = key_stroke(key) {
                    let result = self.scene_host.dispatch_key(stroke, &mut state.components);
                    if result == crate::InteractionResult::Ignored {
                        self.handle_global_key(key, state)?;
                    }
                    self.apply_output_actions(state)?;
                    self.needs_redraw = true;
                }
            }
            Event::Paste(text) => {
                let text = if self.scene_host.focused() == Some(state.composer.id()) {
                    let current = state
                        .components
                        .with(state.composer, |input| input.text().to_string())
                        .unwrap_or_default();
                    state.paste_store.display_text(&current, &text)
                } else {
                    text
                };
                self.scene_host.dispatch_paste(&text, &mut state.components);
                self.apply_output_actions(state)?;
                self.needs_redraw = true;
            }
            Event::Resize(_, _) => self.needs_redraw = true,
            _ => {}
        }
        Ok(())
    }

    fn handle_global_key(&mut self, key: KeyEvent, state: &mut AppState) -> Result<()> {
        let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL;
        if ctrl_c {
            let has_text = state
                .components
                .with(state.composer, |input| !input.is_empty())
                .unwrap_or(false);
            if has_text {
                state.clear_composer();
            } else if state.has_active_turn() {
                self.backend_handler.try_cancel_active_turn()?;
            } else {
                state.request_exit();
            }
            return Ok(());
        }

        if key.code == KeyCode::Esc {
            if state.has_active_turn() {
                self.backend_handler.try_cancel_active_turn()?;
            }
            return Ok(());
        }

        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            state.request_exit();
        }
        Ok(())
    }

    fn apply_output_actions(&mut self, state: &mut AppState) -> Result<()> {
        for action in self
            .scene_host
            .drain_outputs(&state.outputs)
            .map_err(|error| anyhow!(error))?
        {
            match action {
                AppAction::SubmitTurn(text) => {
                    let text = state.expand_submission(text);
                    state.clear_composer();
                    if matches!(
                        self.backend_handler.try_submit_turn(text)?,
                        crate::runtime::backend::SubmitTurnResult::NoBackendAttached
                    ) {
                        state.fail_turn("backend unavailable".to_string())?;
                    }
                }
                AppAction::ToolApproval(decision) => {
                    state.apply_approval_action(decision, &mut self.backend_handler)?;
                }
                AppAction::RequestExit => state.request_exit(),
                AppAction::InterruptActiveTurn => {
                    if state.has_active_turn() {
                        self.backend_handler.try_cancel_active_turn()?;
                    }
                }
                AppAction::CycleReasoningEffort => {
                    state.cycle_reasoning_effort();
                    self.backend_handler.try_cycle_reasoning_effort()?;
                }
            }
        }
        Ok(())
    }

    fn apply_backend_events(
        &mut self,
        events: impl IntoIterator<Item = FrontendEvent>,
        state: &mut AppState,
    ) -> Result<()> {
        for event in events {
            match event {
                FrontendEvent::TurnStarted => {
                    state.turn_started()?;
                }
                FrontendEvent::SteerQueued { text } => state.enqueue_steer(text),
                FrontendEvent::UserMessage { text } => state.submit_user_message(text)?,
                FrontendEvent::AssistantDelta { text } => {
                    self.stream_smoother
                        .push(crate::transcript::SegmentKind::Text, &text);
                }
                FrontendEvent::ThinkingDelta { text } => {
                    self.stream_smoother
                        .push(crate::transcript::SegmentKind::Thinking, &text);
                }
                FrontendEvent::TurnFinished => {
                    self.flush_stream(state)?;
                    state.finish_turn()?
                }
                FrontendEvent::TurnFailed { message } => {
                    self.flush_stream(state)?;
                    state.fail_turn(message)?
                }
                FrontendEvent::TurnCancelled => {
                    self.stream_smoother.clear();
                    state.finish_turn()?
                }
                FrontendEvent::ToolCallStarted {
                    tool_call_id,
                    tool_name,
                    arguments,
                    ..
                } => {
                    self.flush_stream(state)?;
                    state.start_tool_call(tool_call_id, tool_name, arguments)?;
                }
                FrontendEvent::ToolCallUpdated {
                    tool_call_id,
                    update,
                    ..
                } => state.update_tool_call(tool_call_id, update)?,
                FrontendEvent::ToolCallFinished {
                    tool_call_id,
                    is_error,
                } => state.finish_tool_call(tool_call_id, is_error)?,
                FrontendEvent::ToolApprovalRequested {
                    approval_id,
                    tool_call_id,
                    tool_name,
                    arguments,
                    ..
                } => {
                    state.request_tool_approval(approval_id, tool_call_id, tool_name, arguments)?
                }
                FrontendEvent::ToolApprovalResolved {
                    approval_id,
                    tool_call_id,
                    approved,
                    ..
                } => state.resolve_tool_approval(approval_id, tool_call_id, approved)?,
                FrontendEvent::ToolResult {
                    tool_call_id,
                    tool_name,
                    text,
                    details,
                    is_error,
                } => state.push_tool_result(tool_call_id, tool_name, text, details, is_error)?,
                FrontendEvent::ConfigChanged {
                    provider,
                    model_id,
                    reasoning_effort,
                } => state.apply_config(provider, model_id, reasoning_effort),
            }
            self.needs_redraw = true;
        }
        Ok(())
    }

    fn flush_stream_if_ready(&mut self, state: &mut AppState) -> Result<()> {
        if let Some(chunks) = self.stream_smoother.drain_ready(Instant::now()) {
            state.start_assistant_delta(chunks)?;
            self.needs_redraw = true;
        }
        Ok(())
    }

    fn flush_stream(&mut self, state: &mut AppState) -> Result<()> {
        if let Some(chunks) = self.stream_smoother.flush() {
            state.start_assistant_delta(chunks)?;
        }
        Ok(())
    }

    fn next_wait_timeout(&self, state: &AppState) -> Duration {
        let mut timeout = self
            .scene_host
            .next_wait_timeout(Instant::now(), IDLE_POLL_TIMEOUT);
        if self.backend_handler.has_in_flight_turn()
            || self.backend_handler.has_pending_config_sync()
        {
            timeout = timeout.min(BACKEND_POLL_INTERVAL);
        }
        if let Some(smoother) = self.stream_smoother.next_tick_interval() {
            timeout = timeout.min(smoother);
        }
        let _ = state;
        timeout
    }

    fn render(&mut self, terminal: &mut InlineTerminal, state: &mut AppState) -> Result<()> {
        self.needs_redraw = false;
        let mut sink = InlineHistorySink::new(terminal);
        let frame = self
            .scene_host
            .render(&mut state.scene, &mut state.components, &mut sink, |sink| {
                sink.viewport()
            })
            .map_err(|error| anyhow!("{error}"))?;
        drop(sink);
        terminal.draw_scene_frame(&frame)?;
        Ok(())
    }

    fn render_goodbye(
        &mut self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
    ) -> Result<()> {
        state.prepare_goodbye()?;
        self.render(terminal, state)?;
        terminal.position_after_final_frame()
    }
}
