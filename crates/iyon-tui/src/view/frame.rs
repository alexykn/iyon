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
        state.transcript.ensure_render_cache(root.width.max(1));
        root = self.finalize_sealed_host(terminal, state, scrollback, root)?;

        loop {
            state.transcript.ensure_render_cache(root.width.max(1));
            let layout =
                RunningFrameComposer::prepare(renderer, layout_config, state, wrap_cache, root);
            let capacity = layout.conversation_capacity_rows;
            let overflow = state.conversation_row_count().saturating_sub(capacity);
            if overflow == 0 {
                break;
            }

            state.transcript.ensure_render_cache(root.width.max(1));
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

            let host_is_head = state
                .assistant_stream
                .as_ref()
                .is_some_and(|hosted| state.transcript.hosted_unit_is_history_head(hosted.unit_id));
            if !host_is_head {
                break;
            }

            state.prepare_assistant_frame(root.width.max(1), overflow);
            let prepared = state.assistant_frame.take();
            let Some(prepared) = prepared else {
                break;
            };
            if prepared.history.rows.is_empty() {
                break;
            }
            if let Some(hosted) = state.assistant_stream.as_mut() {
                if !state.transcript.hosted_unit_is_history_head(hosted.unit_id) {
                    break;
                }
                debug_assert!(
                    state.transcript.hosted_unit_is_history_head(hosted.unit_id),
                    "HostedStream attempted to leapfrog an earlier transcript unit"
                );
                scrollback.commit_stream_frame(terminal, hosted, prepared)?;
            }
            root = terminal.current_viewport_area()?;
            root = self.finalize_sealed_host(terminal, state, scrollback, root)?;
        }

        state.transcript.ensure_render_cache(root.width.max(1));
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
                    let Some(hosted) = state.assistant_stream.as_ref() else {
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
                        return Ok(root);
                    }
                    debug_assert!(
                        state.transcript.hosted_unit_is_history_head(hosted.unit_id),
                        "pinned HostedStream rows require the global history head"
                    );
                    scrollback.commit_stream_frame(terminal, hosted, prepared)?;
                    root = terminal.current_viewport_area()?;
                }
            }
        }
    }
}
