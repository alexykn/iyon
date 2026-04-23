use std::{
    ops::Range,
    time::{Duration, Instant},
};

use anyhow::Result;
use ratatui::{Frame, layout::Rect, text::Line};

use crate::core::{
    active::{ActiveBehavior, ActivePaneKind},
    input::InputEventHandler,
    layout::{CommitCapacityPolicy, ComputedLayout, LayoutConfig},
    render::{ActiveView, ChatView, InfoView, InputView, Renderable, Renderer},
    scrollback::ScrollbackCommitter,
    state::{AppState, ExitState},
    terminal::InlineTerminal,
};
use crate::util::wrapping::WrapCache;

const EXIT_DRAIN_CHUNK: usize = 256;
const EXIT_GOODBYE: &str = "Goodbye.";
const IDLE_POLL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const SPINNER_TICK_INTERVAL: Duration = Duration::from_millis(80);

#[derive(Debug)]
pub(super) struct App {
    pub(super) input_handler: InputEventHandler,
    pub(super) wrap_cache: WrapCache,
    pub(super) layout: LayoutConfig,
    pub(super) renderer: Renderer,
    pub(super) scrollback: ScrollbackCommitter,
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
            scrollback: ScrollbackCommitter,
            active_ticker: ActiveTicker::new(SPINNER_TICK_INTERVAL),
            needs_redraw: true,
        }
    }
}

#[derive(Debug)]
struct ActiveTicker {
    interval: Duration,
    next_tick: Instant,
    animating: bool,
}

impl ActiveTicker {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_tick: Instant::now() + interval,
            animating: false,
        }
    }

    fn wait_timeout(&mut self, now: Instant, state: &AppState) -> Duration {
        let is_animating = matches!(
            state.active.as_ref().map(|active| active.kind()),
            Some(ActivePaneKind::WorkingSpinner)
        );

        if !is_animating {
            self.animating = false;
            self.next_tick = now + self.interval;
            return IDLE_POLL_TIMEOUT;
        }

        if !self.animating {
            self.animating = true;
            self.next_tick = now + self.interval;
        }

        self.next_tick.saturating_duration_since(now)
    }

    fn tick_if_due(&mut self, now: Instant, state: &mut AppState) -> bool {
        let is_animating = matches!(
            state.active.as_ref().map(|active| active.kind()),
            Some(ActivePaneKind::WorkingSpinner)
        );

        if !is_animating {
            self.animating = false;
            self.next_tick = now + self.interval;
            return false;
        }

        if now < self.next_tick {
            return false;
        }

        state.tick_active();
        self.next_tick = now + self.interval;
        true
    }
}

impl App {
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

        let timeout = self.active_ticker.wait_timeout(Instant::now(), state);
        let input_dirty =
            self.input_handler
                .poll_and_handle(timeout, state, &mut self.wrap_cache)?;

        if !matches!(state.exit_state, ExitState::Running) {
            return Ok(());
        }

        let active_dirty = self.active_ticker.tick_if_due(Instant::now(), state);
        let backend_dirty = self.poll_backend_events(state)?;

        if input_dirty || active_dirty || backend_dirty {
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
        self.spill_active_into_transcript_if_needed(state);

        let root = self.current_root_rect(terminal)?;
        state.transcript.ensure_render_cache(root.width.max(1));

        self.commit_running_overflow_rows(terminal, state, root)?;

        terminal.draw(|frame| self.draw_running(frame, state))?;
        Ok(())
    }

    fn poll_backend_events(&mut self, _state: &mut AppState) -> Result<bool> {
        Ok(false)
    }

    fn spill_active_into_transcript_if_needed(&mut self, state: &mut AppState) {
        if matches!(
            state.active.as_ref().map(|pane| pane.behavior()),
            Some(ActiveBehavior::SpillToTranscript)
        ) {
            // Placeholder: active pane streaming state is not wired yet.
            // ACTIVE_PANE_STREAMING_PLAN.md phase will append canonical spill output here.
        }
    }

    fn step_finalize_active(&mut self, state: &mut AppState) -> Result<()> {
        if let Some(text) = state.take_active_unfrozen_transcript_text() {
            state.append_agent_message(text);
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

        let uncommitted_len = state.transcript.uncommitted_len();
        if uncommitted_len == 0 {
            state.exit_state = ExitState::FinalFrame;
            return Ok(());
        }

        let chunk_len = uncommitted_len.min(EXIT_DRAIN_CHUNK);
        let rows = state.transcript.uncommitted_rows()[..chunk_len].to_vec();
        self.commit_rows_chunked(terminal, &rows)?;
        state.transcript.mark_rows_committed(chunk_len);
        terminal.invalidate_next_draw();

        if state.transcript.uncommitted_len() == 0 {
            state.exit_state = ExitState::FinalFrame;
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
        self.commit_rows_chunked(terminal, &goodbye_rows)?;

        self.set_exit_cursor_position(terminal, root)?;
        state.exit_state = ExitState::Done;
        Ok(())
    }

    fn commit_running_overflow_rows(
        &mut self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
        root: Rect,
    ) -> Result<()> {
        let layout = &self.layout;
        let renderer = &self.renderer;
        // NOTE: We use root width here because layout is vertical-only today, so
        // input area width == root width. If we add horizontal splits/side panes,
        // this should switch to an input-area-specific width source.
        let input_wrap_ranges = self.wrap_cache.input_ranges(
            state.input.text_revision(),
            state.input.text(),
            root.width.max(1),
        );
        let rects = Self::compute_layout(layout, renderer, root, state, input_wrap_ranges);

        let capacity = rects.commit_chat_capacity_rows;
        let uncommitted_len = state.transcript.uncommitted_len();
        if capacity == 0 || uncommitted_len <= capacity {
            return Ok(());
        }

        let overflow = uncommitted_len - capacity;
        let rows = state.transcript.uncommitted_rows()[..overflow].to_vec();
        self.commit_rows_chunked(terminal, &rows)?;
        state.transcript.mark_rows_committed(rows.len());
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
        let size = terminal.inner_mut().size()?;
        let y = viewport
            .y
            .saturating_add(1)
            .min(size.height.saturating_sub(1));
        terminal.inner_mut().set_cursor_position((0, y))?;
        Ok(())
    }

    fn draw_running(&mut self, frame: &mut Frame, state: &mut AppState) {
        let area = frame.area();
        let layout = &self.layout;
        let renderer = &self.renderer;
        let input_wrap_ranges = self.wrap_cache.input_ranges(
            state.input.text_revision(),
            state.input.text(),
            area.width.max(1),
        );
        let rects = Self::compute_layout(layout, renderer, area, state, input_wrap_ranges);
        state.input_content_width = rects.input_area.width.max(1);

        let mut input_view = InputView {
            text: state.input.text(),
            cursor_bytes: state.input.cursor_bytes(),
            scroll_rows: state.scroll,
            wrapped_ranges: input_wrap_ranges,
        };
        let chat_view = ChatView {
            lines: state.transcript.uncommitted_rows(),
        };
        let active_view = ActiveView {
            active: state.active.as_ref(),
        };
        let info_view = InfoView {
            status: &state.info.status,
        };

        state.scroll = renderer.next_input_scroll(&input_view, rects.input_area, state.scroll);
        input_view.scroll_rows = state.scroll;

        renderer.draw_running(
            frame,
            rects,
            &chat_view,
            &active_view,
            &input_view,
            &info_view,
        );
    }

    fn compute_layout(
        layout: &LayoutConfig,
        renderer: &Renderer,
        root: Rect,
        state: &AppState,
        input_wrap_ranges: &[Range<usize>],
    ) -> ComputedLayout {
        let chat_view = ChatView {
            lines: state.transcript.uncommitted_rows(),
        };

        let active_height = if state.active.is_some() { 3 } else { 0 };
        let commit_capacity_policy = match state.active.as_ref().map(|pane| pane.behavior()) {
            Some(ActiveBehavior::OccludeOnly) => CommitCapacityPolicy::OccludeOnly,
            _ => CommitCapacityPolicy::SpillToTranscript,
        };

        Self::compute_layout_for_input(
            layout,
            renderer,
            root,
            state,
            &chat_view,
            input_wrap_ranges,
            active_height,
            commit_capacity_policy,
        )
    }

    fn compute_layout_for_input(
        layout: &LayoutConfig,
        renderer: &Renderer,
        root: Rect,
        state: &AppState,
        chat_view: &ChatView,
        input_wrap_ranges: &[Range<usize>],
        active_height: u16,
        commit_capacity_policy: CommitCapacityPolicy,
    ) -> ComputedLayout {
        let base_cfg = LayoutConfig {
            active_height,
            commit_capacity_policy,
            ..LayoutConfig::default()
        };

        let pass1 = layout.compute(root, &base_cfg);
        let input_view = InputView {
            text: state.input.text(),
            cursor_bytes: state.input.cursor_bytes(),
            scroll_rows: state.scroll,
            wrapped_ranges: input_wrap_ranges,
        };
        let input_height = renderer.desired_height(&input_view, pass1.input_area, "");

        let pass2_cfg = LayoutConfig {
            input_height,
            ..base_cfg.clone()
        };
        let pass2 = layout.compute(root, &pass2_cfg);
        let chat_height = renderer.desired_height(chat_view, pass2.chat_area, "");

        let final_cfg = LayoutConfig {
            chat_height,
            input_height,
            ..pass2_cfg
        };

        layout.compute(root, &final_cfg)
    }

    fn commit_rows_chunked(
        &mut self,
        terminal: &mut InlineTerminal,
        rows: &[Line<'static>],
    ) -> Result<()> {
        const MAX_ROWS_PER_COMMIT: usize = u16::MAX as usize;

        for chunk in rows.chunks(MAX_ROWS_PER_COMMIT) {
            let result = self.scrollback.commit_rows(terminal, chunk)?;
            if let Some(diagnostics) = result.diagnostics {
                return Err(anyhow::anyhow!(
                    "scrollback commit truncated: inserted {} of {} rows (truncated {})",
                    result.rows_inserted,
                    diagnostics.requested_rows,
                    diagnostics.truncated_rows
                ));
            }
            if result.rows_inserted != chunk.len() {
                return Err(anyhow::anyhow!(
                    "scrollback commit mismatch: inserted {} of {} rows",
                    result.rows_inserted,
                    chunk.len()
                ));
            }
        }
        Ok(())
    }

    fn current_root_rect(&mut self, terminal: &mut InlineTerminal) -> Result<Rect> {
        if let Some(area) = terminal.last_viewport_area() {
            return Ok(area);
        }

        let size = terminal.inner_mut().size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }
}
