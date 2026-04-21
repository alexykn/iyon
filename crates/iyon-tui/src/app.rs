use anyhow::Result;
use ratatui::{Frame, layout::Rect, text::Line};

use crate::core::{
    input::InputEventHandler,
    layout::{CommitCapacityPolicy, ComputedLayout, LayoutConfig},
    render::{ChatView, InfoView, InputView, Renderable, Renderer},
    scrollback::ScrollbackCommitter,
    state::{ActiveBehavior, AppState, ExitState},
    terminal::InlineTerminal,
};

const EXIT_DRAIN_CHUNK: usize = 256;
const EXIT_GOODBYE: &str = "Goodbye.";

#[derive(Debug, Default)]
pub(super) struct App {
    pub(super) input_handler: InputEventHandler,
    pub(super) layout: LayoutConfig,
    pub(super) renderer: Renderer,
    pub(super) scrollback: ScrollbackCommitter,
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
        self.spill_active_into_transcript_if_needed(state);

        let root = self.current_root_rect(terminal)?;
        state.sync_chat_from_transcript(root.width.max(1));

        self.commit_running_overflow_rows(terminal, state, root)?;
        state.sync_chat_from_transcript(root.width.max(1));

        terminal.draw(|frame| self.draw_running(frame, state))?;
        self.input_handler.run(state)?;
        Ok(())
    }

    fn spill_active_into_transcript_if_needed(&mut self, state: &mut AppState) {
        if matches!(
            state.active.as_ref().map(|pane| pane.behavior),
            Some(ActiveBehavior::SpillToTranscript)
        ) {
            // Placeholder: active pane streaming state is not wired yet.
            // ACTIVE_PANE_STREAMING_PLAN.md phase will append canonical spill output here.
        }
    }

    fn step_finalize_active(&mut self, state: &mut AppState) -> Result<()> {
        if let Some(active) = state.active.take() {
            match active.behavior {
                ActiveBehavior::SpillToTranscript => {
                    state.append_agent_message(active.spill_text);
                }
                ActiveBehavior::OccludeOnly => {
                    // Drop transient UI-only pane with no transcript mutation.
                }
            }
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
        state.sync_chat_from_transcript(root.width.max(1));

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

        state.sync_chat_from_transcript(root.width.max(1));
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
        state.sync_chat_from_transcript(root.width.max(1));

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
        let rects = self.compute_layout(root, state);

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

        let cursor_y = area.y.saturating_add(2).min(area.bottom().saturating_sub(1));
        frame.set_cursor_position((area.x, cursor_y));
    }

    fn set_exit_cursor_position(&mut self, terminal: &mut InlineTerminal, viewport: Rect) -> Result<()> {
        let size = terminal.inner_mut().size()?;
        let y = viewport
            .y
            .saturating_add(1)
            .min(size.height.saturating_sub(1));
        terminal.inner_mut().set_cursor_position((0, y))?;
        Ok(())
    }

    fn draw_running(&mut self, frame: &mut Frame, state: &mut AppState) {
        let root = frame.area();
        let rects = self.compute_layout(root, state);
        state.input_content_width = rects.input_area.width.max(1);

        let info_lines = self.renderer.format_info_lines(&state.info.status);

        let mut input_view = InputView {
            text: state.input.text(),
            cursor_bytes: state.input.cursor_bytes(),
            scroll_rows: state.scroll,
        };
        let chat_view = ChatView {
            lines: &state.chat.lines,
        };
        let info_view = InfoView { lines: &info_lines };

        state.scroll = self
            .renderer
            .next_input_scroll(&input_view, rects.input_area, state.scroll);
        input_view.scroll_rows = state.scroll;

        self.renderer
            .draw_running(frame, rects, &chat_view, &input_view, &info_view);
    }

    fn draw_exit(&mut self, frame: &mut Frame, state: &mut AppState, goodbye: &str) {
        let root = frame.area();

        let input_view = InputView {
            text: goodbye,
            cursor_bytes: 0,
            scroll_rows: 0,
        };
        let chat_view = ChatView {
            lines: &state.chat.lines,
        };
        let info_lines = self.renderer.format_info_lines(&state.info.status);
        let info_view = InfoView { lines: &info_lines };

        let rects = self.compute_layout_for_input(
            root,
            &chat_view,
            &input_view,
            0,
            CommitCapacityPolicy::SpillToTranscript,
        );

        self.renderer
            .draw_exit(frame, rects, &chat_view, &info_view, goodbye);
    }

    fn compute_layout(&self, root: Rect, state: &AppState) -> ComputedLayout {
        let input_view = InputView {
            text: state.input.text(),
            cursor_bytes: state.input.cursor_bytes(),
            scroll_rows: state.scroll,
        };
        let chat_view = ChatView {
            lines: &state.chat.lines,
        };

        let active_height = if state.active.is_some() { 3 } else { 0 };
        let commit_capacity_policy = match state.active.as_ref().map(|pane| pane.behavior) {
            Some(ActiveBehavior::OccludeOnly) => CommitCapacityPolicy::OccludeOnly,
            _ => CommitCapacityPolicy::SpillToTranscript,
        };

        self.compute_layout_for_input(
            root,
            &chat_view,
            &input_view,
            active_height,
            commit_capacity_policy,
        )
    }

    fn compute_layout_for_input(
        &self,
        root: Rect,
        chat_view: &ChatView,
        input_view: &InputView,
        active_height: u16,
        commit_capacity_policy: CommitCapacityPolicy,
    ) -> ComputedLayout {
        let base_cfg = LayoutConfig::default();

        let input_height = self.renderer.desired_height(input_view, root, "");
        let chat_height = self.renderer.desired_height(chat_view, root, "");

        let final_cfg = LayoutConfig {
            chat_height,
            active_height,
            input_height,
            commit_capacity_policy,
            ..base_cfg
        };

        self.layout.compute(root, &final_cfg)
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
