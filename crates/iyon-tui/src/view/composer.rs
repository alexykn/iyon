use ratatui::layout::Rect;

use crate::{
    input::WrapCache,
    runtime::{AppState, BottomPanelBar, active::ActivePaneState},
    view::{
        layout::{ComputedLayout, LayoutConfig},
        render::{ActiveChromeView, ConversationView, InfoView, InputView, Renderable, Renderer},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct RunningView<'a> {
    pub(crate) layout: ComputedLayout,
    pub(crate) conversation: ConversationView<'a>,
    pub(crate) active_chrome: ActiveChromeView<'a>,
    pub(crate) panel: &'a BottomPanelBar,
    pub(crate) input: InputView<'a>,
    pub(crate) info: InfoView<'a>,
}

#[derive(Debug, Default)]
pub(crate) struct RunningFrameComposer;

impl RunningFrameComposer {
    pub(crate) fn prepare(
        renderer: &Renderer,
        base_layout: &LayoutConfig,
        state: &mut AppState,
        wrap_cache: &mut WrapCache,
        area: Rect,
    ) -> ComputedLayout {
        let input_wrap_ranges = wrap_cache.input_ranges(
            state.input.text_revision(),
            state.input.text(),
            area.width.max(1),
        );
        state.transcript.ensure_render_cache(area.width.max(1));
        state.prepare_assistant_frame(area.width.max(1), 0);
        let layout = Self::compute_layout(base_layout, renderer, area, state, input_wrap_ranges);
        state.input_view.content_width = layout.input_area.width.max(1);

        let input_view = InputView {
            text: state.input.text(),
            cursor_bytes: state.input.cursor_bytes(),
            scroll_rows: state.input_view.scroll_rows,
            wrapped_ranges: input_wrap_ranges,
        };
        state.input_view.scroll_rows = renderer.next_input_scroll(
            &input_view,
            layout.input_area,
            state.input_view.scroll_rows,
        );

        layout
    }

    pub(crate) fn view<'a>(
        layout: ComputedLayout,
        state: &'a AppState,
        wrap_cache: &'a mut WrapCache,
        area: Rect,
    ) -> RunningView<'a> {
        let input_wrap_ranges = wrap_cache.input_ranges(
            state.input.text_revision(),
            state.input.text(),
            area.width.max(1),
        );
        debug_assert!(
            state.assistant_stream.as_ref().map_or(true, |hosted| {
                state.transcript.hosted_unit_is_history_tail(hosted.unit_id)
            }),
            "hosted conversation rows must follow resident transcript rows"
        );

        RunningView {
            layout,
            conversation: ConversationView {
                transcript_rows: state.transcript.uncommitted_rows(),
                hosted_rows: state.assistant_live_rows(),
            },
            active_chrome: ActiveChromeView {
                active: state.active.as_ref(),
            },
            panel: &state.bottom_panel,
            input: InputView {
                text: state.input.text(),
                cursor_bytes: state.input.cursor_bytes(),
                scroll_rows: state.input_view.scroll_rows,
                wrapped_ranges: input_wrap_ranges,
            },
            info: InfoView {
                provider: &state.info.provider,
                model_id: &state.info.model_id,
                reasoning_effort: state.info.reasoning_effort,
                status: &state.info.status,
            },
        }
    }

    fn compute_layout(
        base_layout: &LayoutConfig,
        renderer: &Renderer,
        root: Rect,
        state: &AppState,
        input_wrap_ranges: &[std::ops::Range<usize>],
    ) -> ComputedLayout {
        let conversation_rows = state.conversation_row_count();
        let conversation_height = u16::try_from(conversation_rows).unwrap_or(u16::MAX);
        let conversation_gap_height = u16::from(conversation_rows > 0);
        let active_chrome_height = state
            .active
            .as_ref()
            .map_or(0, ActivePaneState::desired_height);
        let panel_height = state.bottom_panel.desired_height(root.width.max(1));

        Self::compute_layout_for_input(
            base_layout,
            renderer,
            root,
            state,
            input_wrap_ranges,
            conversation_height,
            conversation_gap_height,
            active_chrome_height,
            panel_height,
        )
    }

    fn compute_layout_for_input(
        base_layout: &LayoutConfig,
        renderer: &Renderer,
        root: Rect,
        state: &AppState,
        input_wrap_ranges: &[std::ops::Range<usize>],
        conversation_height: u16,
        conversation_gap_height: u16,
        active_chrome_height: u16,
        panel_height: u16,
    ) -> ComputedLayout {
        let base_cfg = LayoutConfig {
            conversation_height,
            conversation_gap_height,
            active_chrome_height,
            panel_height,
            ..base_layout.clone()
        };

        let pass1 = base_cfg.compute(root);
        let input_view = InputView {
            text: state.input.text(),
            cursor_bytes: state.input.cursor_bytes(),
            scroll_rows: state.input_view.scroll_rows,
            wrapped_ranges: input_wrap_ranges,
        };
        let input_height = renderer.desired_height(&input_view, pass1.input_area);
        let fixed_height = conversation_gap_height
            .saturating_add(active_chrome_height)
            .saturating_add(panel_height)
            .saturating_add(input_height)
            .saturating_add(base_cfg.info_height);
        let max_conversation_height = root.height.saturating_sub(fixed_height);
        let conversation_height = conversation_height.min(max_conversation_height);

        let final_cfg = LayoutConfig {
            conversation_height,
            input_height,
            ..base_cfg
        };
        final_cfg.compute(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::active::ToolActiveStatus;

    #[test]
    fn ownership_handoff_preserves_conversation_geometry_and_rows() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(
            crate::transcript::SegmentKind::Text,
            "# Heading\n- one\n- two\n\nwide 漢 and combining e\u{301} text".to_string(),
        )]);
        let root = Rect::new(0, 0, 80, 20);
        let renderer = Renderer;
        let mut wrap_cache = WrapCache::default();
        let before = RunningFrameComposer::prepare(
            &renderer,
            &LayoutConfig::default(),
            &mut state,
            &mut wrap_cache,
            root,
        );
        let before_rows = state.assistant_live_rows().to_vec();

        state.finish_active_turn();
        assert_eq!(
            state.finalize_sealed_assistant_stream(),
            crate::runtime::FinalizeResult::HandedOff
        );
        let after = RunningFrameComposer::prepare(
            &renderer,
            &LayoutConfig::default(),
            &mut state,
            &mut wrap_cache,
            root,
        );
        let after_rows = state.transcript.uncommitted_rows().to_vec();

        assert_eq!(before_rows, after_rows);
        assert_eq!(before.spacer_area.height, after.spacer_area.height);
        assert_eq!(before.conversation_area.y, after.conversation_area.y);
        assert_eq!(
            before.conversation_area.height,
            after.conversation_area.height
        );
        assert_eq!(
            before.conversation_area.bottom(),
            after.conversation_area.bottom()
        );
        assert_eq!(before.conversation_gap_area, after.conversation_gap_area);
        assert_eq!(before.input_area.y, after.input_area.y);
    }

    #[test]
    fn layout_gap_is_not_counted_as_conversation_overflow() {
        let mut state = AppState::default();
        state.append_timeline_item(crate::transcript::TimelineItem::UserMessage {
            text: "user".to_string(),
        });

        let root = Rect::new(0, 0, 80, 8);
        let renderer = Renderer;
        let mut wrap_cache = WrapCache::default();
        let layout = RunningFrameComposer::prepare(
            &renderer,
            &LayoutConfig::default(),
            &mut state,
            &mut wrap_cache,
            root,
        );

        assert_eq!(state.transcript.uncommitted_len(), 3);
        assert_eq!(state.conversation_row_count(), 3);
        assert_eq!(layout.conversation_capacity_rows, 3);
        assert_eq!(layout.conversation_gap_area.height, 1);
    }

    #[test]
    fn bottom_panel_pressure_reduces_only_conversation_capacity() {
        let mut state = AppState::default();
        state.receive_stream_segments(vec![(
            crate::transcript::SegmentKind::Text,
            (0..30)
                .map(|index| format!("resident content {index}"))
                .collect::<Vec<_>>()
                .join("\n"),
        )]);
        state.finish_active_turn();
        assert_eq!(
            state.finalize_sealed_assistant_stream(),
            crate::runtime::FinalizeResult::HandedOff
        );

        let root = Rect::new(0, 0, 80, 20);
        let renderer = Renderer;
        let mut wrap_cache = WrapCache::default();
        let before = RunningFrameComposer::prepare(
            &renderer,
            &LayoutConfig::default(),
            &mut state,
            &mut wrap_cache,
            root,
        );
        state.enqueue_steer("queued".to_string());
        let after = RunningFrameComposer::prepare(
            &renderer,
            &LayoutConfig::default(),
            &mut state,
            &mut wrap_cache,
            root,
        );

        assert!(after.conversation_capacity_rows < before.conversation_capacity_rows);
        assert_eq!(after.input_area.height, before.input_area.height);
        assert_eq!(after.info_area.height, before.info_area.height);
    }

    #[test]
    fn active_chrome_reserves_height_without_assistant_host() {
        let mut state = AppState::default();
        state.active = Some(ActivePaneState::working_spinner());

        let layout = RunningFrameComposer::compute_layout(
            &LayoutConfig::default(),
            &Renderer,
            Rect::new(0, 0, 80, 30),
            &state,
            &[0..0],
        );

        assert_eq!(layout.active_chrome_area.height, 3);

        state.active = Some(ActivePaneState::Tool {
            tool_name: "search".to_string(),
            status: ToolActiveStatus::Running,
            detail: None,
        });
        let layout = RunningFrameComposer::compute_layout(
            &LayoutConfig::default(),
            &Renderer,
            Rect::new(0, 0, 80, 30),
            &state,
            &[0..0],
        );
        assert_eq!(layout.active_chrome_area.height, 3);
    }
}
