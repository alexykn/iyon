//! INTERNAL PRESENTATION MECHANICS.
//!
//! Owns the existing multi-pass convergence between semantic presentation,
//! active overflow, transcript capacity, native scrollback, and final draw.
//! Feature code must not orchestrate these phases independently.

use anyhow::Result;
use ratatui::layout::Rect;

use crate::{
    input::WrapCache,
    runtime::{AppController, AppState},
    scrollback::ScrollbackCoordinator,
    terminal::InlineTerminal,
};

use super::{LayoutConfig, Renderer, RunningFrameComposer};

/// INTERNAL PRESENTATION MECHANICS.
///
/// The algorithm is intentionally the current proven sequence; this type only
/// removes it from `App` so future frame changes stay below feature code.
#[derive(Debug, Default)]
pub(crate) struct FrameCoordinator;

impl FrameCoordinator {
    pub(crate) fn render_running(
        &self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
        renderer: &Renderer,
        layout_config: &LayoutConfig,
        wrap_cache: &mut WrapCache,
        scrollback: &mut ScrollbackCoordinator,
        controller: &mut AppController,
        root: Rect,
    ) -> Result<()> {
        state.transcript.ensure_render_cache(root.width.max(1));

        let mut layout =
            RunningFrameComposer::prepare(renderer, layout_config, state, wrap_cache, root);

        if controller
            .spill_active_into_transcript_if_needed(state, usize::from(layout.active_area.height))
        {
            state.transcript.ensure_render_cache(root.width.max(1));
            layout =
                RunningFrameComposer::prepare(renderer, layout_config, state, wrap_cache, root);
        }

        scrollback.commit_running_overflow(
            terminal,
            &mut state.transcript,
            layout.commit_chat_capacity_rows,
        )?;

        let root = terminal.current_viewport_area()?;
        state.transcript.ensure_render_cache(root.width.max(1));
        layout = RunningFrameComposer::prepare(renderer, layout_config, state, wrap_cache, root);

        terminal.draw(|frame| {
            let view = RunningFrameComposer::view(layout, state, wrap_cache, frame.area());
            renderer.draw_running(frame, &view);
        })?;
        Ok(())
    }
}
