//! INTERNAL PRESENTATION MECHANICS.
//!
//! Owns the ordered visual conversation surface, loss-aware native-history
//! promotion, and the final single draw. Feature code must not orchestrate
//! these phases independently.

use anyhow::Result;
use ratatui::layout::Rect;

use crate::{
    input::WrapCache,
    runtime::{AppState, FinalizeResult},
    scrollback::ScrollbackCoordinator,
    terminal::InlineTerminal,
};

use super::{LayoutConfig, Renderer, RunningFrameComposer};

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
        root: Rect,
    ) -> Result<()> {
        let mut root = root;
        state.transcript.ensure_render_cache(root.width);
        root = self.finalize_sealed_host(terminal, state, scrollback, root)?;

        loop {
            state.transcript.ensure_render_cache(root.width);
            let layout =
                RunningFrameComposer::prepare(renderer, layout_config, state, wrap_cache, root);
            let capacity = layout.conversation_capacity_rows;
            let overflow = state.conversation_row_count().saturating_sub(capacity);
            if overflow == 0 {
                break;
            }

            state.transcript.ensure_render_cache(root.width);
            let transcript_rows = overflow.min(state.transcript.committable_len());
            if transcript_rows > 0 {
                let inserted = scrollback.commit_transcript_prefix(
                    terminal,
                    &mut state.transcript,
                    transcript_rows,
                )?;
                if inserted == 0 {
                    break;
                }
                root = terminal.current_viewport_area()?;
                root = self.finalize_sealed_host(terminal, state, scrollback, root)?;
                continue;
            }

            let Some(unit_id) = state.streaming_ref().map(|hosted| hosted.unit_id) else {
                break;
            };
            if !state.transcript.hosted_unit_is_history_head(unit_id) {
                break;
            }

            state.prepare_assistant_frame(root.width, overflow);
            let prepared = state.take_assistant_frame();
            let Some(prepared) = prepared else {
                break;
            };
            if prepared.history.rows.is_empty() {
                break;
            }
            let Some(hosted) = state.streaming_mut() else {
                break;
            };
            debug_assert_eq!(hosted.unit_id, unit_id);
            scrollback.commit_stream_frame(terminal, hosted, prepared)?;
            root = terminal.current_viewport_area()?;
            root = self.finalize_sealed_host(terminal, state, scrollback, root)?;
        }

        state.transcript.ensure_render_cache(root.width);
        let layout =
            RunningFrameComposer::prepare(renderer, layout_config, state, wrap_cache, root);
        let view = RunningFrameComposer::view(layout, state, wrap_cache, root);
        terminal.draw(|frame| renderer.draw_running(frame, &view))?;
        Ok(())
    }

    /// Finalizes a sealed mutable host without drawing an intermediate collapsed
    /// frame. Only a pinned physical atomic remainder may force a terminal write.
    fn finalize_sealed_host(
        &self,
        terminal: &mut InlineTerminal,
        state: &mut AppState,
        scrollback: &mut ScrollbackCoordinator,
        mut root: Rect,
    ) -> Result<Rect> {
        loop {
            match state.finalize_sealed_assistant_stream() {
                FinalizeResult::Nothing
                | FinalizeResult::HandedOff
                | FinalizeResult::FullyNative => return Ok(root),
                FinalizeResult::NeedsPinnedDrain => {
                    let Some(hosted) = state.streaming_ref() else {
                        return Ok(root);
                    };
                    let unit_id = hosted.unit_id;
                    let blocking_rows = hosted.handoff_blocking_rows();
                    if blocking_rows == 0 {
                        return Err(anyhow::anyhow!(
                            "sealed HostedStream cannot hand off its physical partial state"
                        ));
                    }
                    if !state.transcript.hosted_unit_is_history_head(unit_id) {
                        return Ok(root);
                    }
                    debug_assert!(
                        state.transcript.hosted_unit_is_history_head(unit_id),
                        "pinned HostedStream rows require the global history head"
                    );
                    state.prepare_assistant_frame(root.width, blocking_rows);
                    let prepared = state.take_assistant_frame().ok_or_else(|| {
                        anyhow::anyhow!("sealed HostedStream produced no pinned drain frame")
                    })?;
                    let Some(hosted) = state.streaming_mut() else {
                        return Err(anyhow::anyhow!(
                            "HostedStream disappeared during pinned drain"
                        ));
                    };
                    debug_assert_eq!(hosted.unit_id, unit_id);
                    scrollback.commit_stream_frame(terminal, hosted, prepared)?;
                    root = terminal.current_viewport_area()?;
                }
            }
        }
    }
}
