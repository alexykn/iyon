use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::{Frame, layout::Rect, text::Line};

use crate::{
    input::{InputEditor, InputEventHandler, WrapCache},
    runtime::{ActiveTicker, AppController, AppState, BackendEventHandler, ExitState},
    scrollback::{FlushResult, ScrollbackCoordinator},
    terminal::InlineTerminal,
    view::{LayoutConfig, Renderer, RunningFrameComposer},
};

const EXIT_DRAIN_CHUNK: usize = 256;
const EXIT_GOODBYE: &str = "Goodbye.";
const IDLE_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const SPINNER_TICK_INTERVAL: Duration = Duration::from_millis(80);
const BACKEND_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug)]
pub(super) struct App {
    pub(super) input_handler: InputEventHandler,
    pub(super) wrap_cache: WrapCache,
    pub(super) layout: LayoutConfig,
    pub(super) renderer: Renderer,
    pub(super) scrollback: ScrollbackCoordinator,
    backend_handler: BackendEventHandler,
    controller: AppController,
    active_ticker: ActiveTicker,
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
            backend_handler: BackendEventHandler::default(),
            controller: AppController,
            active_ticker: ActiveTicker::new(SPINNER_TICK_INTERVAL),
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
        let input_actions = self
            .input_handler
            .poll_and_handle(timeout, &mut input_editor)?;
        let input_dirty =
            self.controller
                .apply_all(input_actions, state, &mut self.backend_handler)?;

        if !matches!(state.exit_state, ExitState::Running) {
            return Ok(());
        }

        let backend_events = self.backend_handler.drain_events()?;
        let backend_dirty = self
            .controller
            .apply_backend_events(backend_events, state)?;
        let active_dirty = self
            .active_ticker
            .tick_if_due(Instant::now(), state.active.as_mut());

        if input_dirty || backend_dirty || active_dirty {
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
        self.controller
            .spill_active_into_transcript_if_needed(state);

        let root = self.current_root_rect(terminal)?;
        state.transcript.ensure_render_cache(root.width.max(1));

        let layout = RunningFrameComposer::prepare(
            &self.renderer,
            &self.layout,
            state,
            &mut self.wrap_cache,
            root,
        );

        self.scrollback.commit_running_overflow(
            terminal,
            &mut state.transcript,
            layout.commit_chat_capacity_rows,
        )?;

        terminal.draw(|frame| {
            let view =
                RunningFrameComposer::view(layout, state, &mut self.wrap_cache, frame.area());
            self.renderer.draw_running(frame, &view);
        })?;
        Ok(())
    }

    fn next_wait_timeout(&mut self, state: &AppState) -> Duration {
        let active_timeout = self.active_ticker.wait_timeout(
            Instant::now(),
            state.active.as_ref(),
            IDLE_POLL_TIMEOUT,
        );
        if self.backend_handler.has_in_flight_turn() {
            active_timeout.min(BACKEND_POLL_INTERVAL)
        } else {
            active_timeout
        }
    }

    fn step_finalize_active(&mut self, state: &mut AppState) -> Result<()> {
        if let Some(text) = state.take_active_unfrozen_transcript_text() {
            state.append_assistant_message(text);
        }

        state.exit_state = ExitState::FlushHistory;
        Ok(())
    }

    fn step_flush_history(
        &mut self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
    ) -> Result<()> {
        let root = self.current_root_rect(terminal)?;
        state.transcript.ensure_render_cache(root.width.max(1));

        match self.scrollback.flush_history_chunk(
            terminal,
            &mut state.transcript,
            EXIT_DRAIN_CHUNK,
        )? {
            FlushResult::Done => state.exit_state = ExitState::FinalFrame,
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

        if state.transcript.uncommitted_len() != 0 {
            return Err(anyhow::anyhow!(
                "shutdown final frame reached with {} uncommitted transcript rows",
                state.transcript.uncommitted_len()
            ));
        }

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
        if let Some(area) = terminal.last_viewport_area() {
            return Ok(area);
        }

        terminal.size()
    }
}
