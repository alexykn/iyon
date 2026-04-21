use anyhow::Result;
use ratatui::{DefaultTerminal, Frame, layout::Rect};

use crate::core::{
    input::InputEventHandler,
    layout::LayoutConfig,
    render::{ChatView, InfoView, InputView, Renderable, Renderer, SpacerView},
    scrollback::ScrollbackCommitter,
    state::AppState,
};

#[derive(Debug, Default)]
pub(super) struct App {
    pub(super) input_handler: InputEventHandler,
    pub(super) layout: LayoutConfig,
    pub(super) renderer: Renderer,
    pub(super) scrollback: ScrollbackCommitter,
}

impl App {
    pub(super) fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut state = AppState::default();

        while !state.exit {
            self.commit_scrolled_out_lines(terminal, &mut state)?;
            terminal.draw(|frame| self.draw(frame, &mut state))?;
            self.input_handler.run(&mut state)?;
        }
        Ok(())
    }

    fn commit_scrolled_out_lines(
        &mut self,
        terminal: &mut DefaultTerminal,
        state: &mut AppState,
    ) -> Result<()> {
        let size = terminal.size()?;
        let root = Rect::new(0, 0, size.width, size.height);

        let base_cfg = LayoutConfig::default();
        let pass1 = self.layout.compute(root, &base_cfg);

        let input_view = InputView {
            text: state.input.text(),
            cursor_bytes: state.input.cursor_bytes(),
            scroll_rows: state.scroll,
        };
        let chat_view = ChatView {
            lines: &state.chat.lines,
        };

        let input_height = self
            .renderer
            .desired_height(&input_view, pass1.input_area, "");
        let chat_height = self
            .renderer
            .desired_height(&chat_view, pass1.chat_area, "");
        let final_cfg = LayoutConfig {
            chat_height,
            input_height,
            ..base_cfg
        };
        let rects = self.layout.compute(root, &final_cfg);

        let capacity = usize::from(rects.chat_area.height);
        if capacity == 0 || state.chat.lines.len() <= capacity {
            return Ok(());
        }

        let overflow = state.chat.lines.len() - capacity;
        state
            .pending_scrollback
            .extend(state.chat.lines.drain(0..overflow));

        self.scrollback
            .commit_pending(terminal, &mut state.pending_scrollback)?;
        Ok(())
    }

    fn draw(&self, frame: &mut Frame, state: &mut AppState) {
        let root = frame.area();
        let spacer_view = SpacerView;

        let base_cfg = LayoutConfig::default();
        let pass1 = self.layout.compute(root, &base_cfg);
        state.input_content_width = pass1.input_area.width.max(1);

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

        let input_height = self
            .renderer
            .desired_height(&input_view, pass1.input_area, "");
        let chat_height = self
            .renderer
            .desired_height(&chat_view, pass1.chat_area, "");

        let final_cfg = LayoutConfig {
            chat_height,
            input_height,
            ..base_cfg
        };
        let rects = self.layout.compute(root, &final_cfg);

        state.scroll = self
            .renderer
            .next_input_scroll(&input_view, rects.input_area, state.scroll);
        input_view.scroll_rows = state.scroll;

        self.renderer.render(&spacer_view, frame, rects.spacer_area);
        self.renderer.render(&chat_view, frame, rects.chat_area);
        self.renderer.render(&input_view, frame, rects.input_area);
        self.renderer.render(&info_view, frame, rects.info_area);
    }
}
