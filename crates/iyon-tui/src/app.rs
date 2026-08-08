use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::{Frame, layout::Rect, text::Line};

use crate::transcript::SegmentKind;
use crate::{
    input::{InputEditor, InputEventHandler, WrapCache},
    runtime::{
        ActiveTicker, AppController, AppState, BackendEventHandler, ExitState, FinalizeResult,
        FrontendEvent, StreamSmoother,
    },
    scrollback::{FlushResult, ScrollbackCoordinator},
    terminal::InlineTerminal,
    view::{FrameCoordinator, LayoutConfig, Renderer},
};

const EXIT_DRAIN_CHUNK: usize = 256;
const EXIT_GOODBYE: &str = "Goodbye.";
const IDLE_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const SPINNER_TICK_INTERVAL: Duration = Duration::from_millis(80);
const BACKEND_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) struct App {
    pub(super) input_handler: InputEventHandler,
    pub(super) wrap_cache: WrapCache,
    pub(super) layout: LayoutConfig,
    pub(super) renderer: Renderer,
    pub(super) scrollback: ScrollbackCoordinator,
    pub(super) frame_coordinator: FrameCoordinator,
    backend_handler: BackendEventHandler,
    controller: AppController,
    active_ticker: ActiveTicker,
    stream_smoother: StreamSmoother,
    needs_redraw: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input_handler: InputEventHandler,
            wrap_cache: WrapCache::default(),
            layout: LayoutConfig::default(),
            renderer: Renderer,
            scrollback: ScrollbackCoordinator::default(),
            frame_coordinator: FrameCoordinator,
            backend_handler: BackendEventHandler::default(),
            controller: AppController,
            active_ticker: ActiveTicker::new(SPINNER_TICK_INTERVAL),
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

        // Apply any core state already available (e.g. the initial ConfigChanged)
        // before the first frame so the status bar is populated immediately rather
        // than waiting for the first user input to trigger a drain.
        let initial_events = self.backend_handler.drain_events()?;
        self.apply_backend_events(initial_events, &mut state);

        loop {
            self.step(terminal, &mut state)?;
            if matches!(state.exit_state, ExitState::Done) {
                break;
            }
        }

        Ok(())
    }

    fn step(&mut self, terminal: &mut InlineTerminal, state: &mut AppState) -> Result<()> {
        match state.exit_state {
            ExitState::Running => self.step_running(terminal, state),
            ExitState::Requested => {
                state.exit_state = ExitState::FinalizeActive;
                Ok(())
            }
            ExitState::FinalizeActive => self.step_finalize_active(state),
            ExitState::FlushHistory => self.step_flush_history(terminal, state),
            ExitState::FinalFrame => self.step_final_frame(terminal, state),
            ExitState::Done => Ok(()),
        }
    }

    fn step_running(&mut self, terminal: &mut InlineTerminal, state: &mut AppState) -> Result<()> {
        if self.needs_redraw {
            self.draw_dirty_running_frame(terminal, state)?;
            return Ok(());
        }

        let timeout = self.next_wait_timeout(state);
        let mut input_editor = InputEditor::new(
            &mut state.input,
            state.input_view.content_width,
            &mut self.wrap_cache,
        );
        let input_actions = self.input_handler.poll_and_handle(
            timeout,
            &mut input_editor,
            state.pending_tool_approval.is_some(),
            &mut state.bottom_panel,
        )?;
        let input_dirty =
            self.controller
                .apply_all(input_actions, state, &mut self.backend_handler)?;

        if !matches!(state.exit_state, ExitState::Running) {
            return Ok(());
        }

        let backend_events = self.backend_handler.drain_events()?;
        let backend_dirty = self.apply_backend_events(backend_events, state);

        let now = Instant::now();
        let active_dirty = self.active_ticker.tick_if_due(now, state.active.as_mut());
        let smoothed_dirty = self.drain_stream_smoother(state, now);

        if input_dirty || backend_dirty || active_dirty || smoothed_dirty {
            self.draw_dirty_running_frame(terminal, state)?;
        }

        Ok(())
    }

    fn draw_dirty_running_frame(
        &mut self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
    ) -> Result<()> {
        self.needs_redraw = false;

        let root = self.current_root_rect(terminal)?;
        self.frame_coordinator.render_running(
            terminal,
            state,
            &self.renderer,
            &self.layout,
            &mut self.wrap_cache,
            &mut self.scrollback,
            root,
        )?;
        Ok(())
    }

    fn apply_backend_events(
        &mut self,
        events: impl IntoIterator<Item = FrontendEvent>,
        state: &mut AppState,
    ) -> bool {
        let mut dirty = false;
        for event in events {
            dirty |= self.apply_backend_event(event, state);
        }
        dirty
    }

    fn apply_backend_event(&mut self, event: FrontendEvent, state: &mut AppState) -> bool {
        match event {
            FrontendEvent::TurnStarted => {
                state.start_working_pane();
                true
            }
            FrontendEvent::SteerQueued { text } => {
                state.enqueue_steer(text);
                true
            }
            FrontendEvent::UserMessage { text } => {
                state.submit_user_message(text);
                true
            }
            FrontendEvent::AssistantDelta { text } => {
                // Backend chunks are intentionally not appended directly to the visible
                // active stream. They enter a presentation buffer first, then the event
                // loop drains that buffer at a steady cadence for smoother streaming.
                self.stream_smoother.push(SegmentKind::Text, &text);
                state.start_working_pane();
                true
            }
            FrontendEvent::ThinkingDelta { text } => {
                self.stream_smoother.push(SegmentKind::Thinking, &text);
                state.start_working_pane();
                true
            }
            FrontendEvent::TurnFinished => {
                // Flush before closing the active turn; otherwise buffered text would be
                // appended after TranscriptState has rebuilt the assistant message as final.
                self.flush_stream_smoother(state);
                state.finish_active_turn();
                true
            }
            FrontendEvent::TurnFailed { message } => {
                self.flush_stream_smoother(state);
                state.fail_active_turn(message);
                true
            }
            FrontendEvent::TurnCancelled => {
                self.stream_smoother.clear();
                state.finish_active_turn();
                true
            }
            FrontendEvent::ToolCallStarted {
                tool_call_id,
                tool_name,
                arguments,
                ..
            } => {
                self.flush_stream_smoother(state);
                state.start_tool_call(tool_call_id, tool_name, arguments);
                true
            }
            FrontendEvent::ToolCallUpdated {
                tool_name, update, ..
            } => {
                state.update_tool_call(tool_name, update);
                true
            }
            FrontendEvent::ToolCallFinished {
                tool_call_id,
                is_error,
            } => {
                state.finish_tool_call(tool_call_id, is_error);
                true
            }
            FrontendEvent::ToolApprovalRequested {
                approval_id,
                tool_call_id,
                tool_name,
                arguments,
                ..
            } => {
                state.request_tool_approval(approval_id, tool_call_id, tool_name, arguments);
                true
            }
            FrontendEvent::ToolApprovalResolved {
                approval_id,
                tool_call_id,
                approved,
                ..
            } => {
                state.resolve_tool_approval(approval_id, tool_call_id, approved);
                true
            }
            FrontendEvent::ToolResult {
                tool_call_id,
                tool_name,
                text,
                details,
                is_error,
            } => {
                state.push_tool_result(tool_call_id, tool_name, text, details, is_error);
                true
            }
            FrontendEvent::ConfigChanged {
                provider,
                model_id,
                reasoning_effort,
            } => {
                state.apply_config(provider, model_id, reasoning_effort);
                true
            }
        }
    }

    fn drain_stream_smoother(&mut self, state: &mut AppState, now: Instant) -> bool {
        let Some(chunks) = self.stream_smoother.drain_ready(now) else {
            return false;
        };

        state.receive_stream_segments(chunks);
        true
    }

    fn flush_stream_smoother(&mut self, state: &mut AppState) -> bool {
        let Some(chunks) = self.stream_smoother.flush() else {
            return false;
        };

        state.receive_stream_segments(chunks);
        true
    }

    fn next_wait_timeout(&mut self, state: &AppState) -> Duration {
        let active_timeout = self.active_ticker.wait_timeout(
            Instant::now(),
            state.active.as_ref(),
            IDLE_POLL_TIMEOUT,
        );
        let mut timeout = if self.backend_handler.has_in_flight_turn()
            || self.backend_handler.has_pending_config_sync()
        {
            active_timeout.min(BACKEND_POLL_INTERVAL)
        } else {
            active_timeout
        };

        // The backend may deliver text in chunky bursts. While the smoother has buffered
        // text, keep the event loop waking at its animation cadence so pending characters
        // can be released even if there is no input/backend event to wake us.
        if let Some(smoother_timeout) = self.stream_smoother.next_tick_interval() {
            timeout = timeout.min(smoother_timeout);
        }

        timeout
    }

    fn step_finalize_active(&mut self, state: &mut AppState) -> Result<()> {
        self.flush_stream_smoother(state);
        state.finish_active_turn();
        state.exit_state = ExitState::FlushHistory;
        Ok(())
    }

    fn step_flush_history(
        &mut self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
    ) -> Result<()> {
        let mut root = self.current_root_rect(terminal)?;
        state.transcript.ensure_render_cache(root.width.max(1));

        loop {
            match state.finalize_sealed_assistant_stream() {
                FinalizeResult::Nothing
                | FinalizeResult::HandedOff
                | FinalizeResult::FullyNative => break,
                FinalizeResult::NeedsPinnedDrain => {
                    let Some(hosted) = state.assistant_stream.as_ref() else {
                        break;
                    };
                    let blocking_rows = hosted.handoff_blocking_rows();
                    if blocking_rows == 0 {
                        return Err(anyhow::anyhow!(
                            "sealed HostedStream cannot hand off its physical partial state"
                        ));
                    }
                    if !state.transcript.hosted_unit_is_history_head(hosted.unit_id) {
                        break;
                    }
                    debug_assert!(
                        state.transcript.hosted_unit_is_history_head(hosted.unit_id),
                        "pinned HostedStream rows require the global history head"
                    );
                    state.prepare_assistant_frame(root.width.max(1), blocking_rows);
                    let prepared = state.assistant_frame.take().ok_or_else(|| {
                        anyhow::anyhow!("sealed HostedStream produced no pinned drain frame")
                    })?;
                    let Some(hosted) = state.assistant_stream.as_mut() else {
                        return Err(anyhow::anyhow!(
                            "HostedStream disappeared during pinned drain"
                        ));
                    };
                    if !state.transcript.hosted_unit_is_history_head(hosted.unit_id) {
                        break;
                    }
                    self.scrollback
                        .commit_stream_frame(terminal, hosted, prepared)?;
                    root = self.current_root_rect(terminal)?;
                }
            }
        }
        state.transcript.ensure_render_cache(root.width.max(1));

        match self.scrollback.flush_history_chunk(
            terminal,
            &mut state.transcript,
            EXIT_DRAIN_CHUNK,
        )? {
            FlushResult::Done | FlushResult::Blocked => state.exit_state = ExitState::FinalFrame,
            FlushResult::More => {}
        }
        Ok(())
    }

    fn step_final_frame(
        &mut self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
    ) -> Result<()> {
        let root = self.current_root_rect(terminal)?;
        state.transcript.ensure_render_cache(root.width.max(1));

        terminal.draw(|frame| self.draw_exit_anchor(frame))?;

        let goodbye_rows = [Line::from(""), Line::from(""), Line::from(EXIT_GOODBYE)];
        self.scrollback
            .commit_rows_checked(terminal, &goodbye_rows)?;

        self.set_exit_cursor_position(terminal, root)?;
        state.exit_state = ExitState::Done;
        Ok(())
    }

    fn draw_exit_anchor(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Paragraph::default(), area);

        if area.height == 0 {
            return;
        }

        let cursor_y = area
            .y
            .saturating_add(2)
            .min(area.bottom().saturating_sub(1));
        frame.set_cursor_position((area.x, cursor_y));
    }

    fn set_exit_cursor_position(
        &mut self,
        terminal: &mut InlineTerminal,
        viewport: Rect,
    ) -> Result<()> {
        let size = terminal.size()?;
        let y = viewport
            .y
            .saturating_add(1)
            .min(size.height.saturating_sub(1));
        terminal.set_cursor_position((0, y))?;
        Ok(())
    }

    fn current_root_rect(&mut self, terminal: &mut InlineTerminal) -> Result<Rect> {
        terminal.current_viewport_area()
    }
}
