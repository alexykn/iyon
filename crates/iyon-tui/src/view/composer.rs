use ratatui::layout::Rect;

use crate::{
    input::WrapCache,
    runtime::{
        AppState, BottomPanelBar,
        active::{ActiveBehavior, ActivePaneState},
    },
    view::{
        layout::{CommitCapacityPolicy, ComputedLayout, LayoutConfig},
        render::{ActiveView, ChatView, InfoView, InputView, Renderable, Renderer},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct RunningView<'a> {
    pub(crate) layout: ComputedLayout,
    pub(crate) chat: ChatView<'a>,
    pub(crate) active: ActiveView<'a>,
    pub(crate) panel: &'a BottomPanelBar,
    pub(crate) input: InputView<'a>,
    pub(crate) info: InfoView<'a>,
}

const MIN_ACTIVE_HEIGHT: u16 = 3;
const MIN_CHAT_HEIGHT: u16 = 1;

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
        if let Some(active) = state.active.as_mut() {
            active.ensure_stream_render_cache(area.width.max(1));
        }
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

        RunningView {
            layout,
            chat: ChatView {
                lines: state.transcript.uncommitted_rows(),
            },
            active: ActiveView {
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
        let chat_view = ChatView {
            lines: state.transcript.uncommitted_rows(),
        };

        let active_height = state
            .active
            .as_ref()
            .map_or(0, ActivePaneState::desired_height);
        let commit_capacity_policy = match state
            .active
            .as_ref()
            .map(super::super::runtime::active::ActivePaneState::behavior)
        {
            Some(ActiveBehavior::OccludeOnly) => CommitCapacityPolicy::OccludeOnly,
            _ => CommitCapacityPolicy::SpillToTranscript,
        };

        Self::compute_layout_for_input(
            base_layout,
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
        base_layout: &LayoutConfig,
        renderer: &Renderer,
        root: Rect,
        state: &AppState,
        chat_view: &ChatView,
        input_wrap_ranges: &[std::ops::Range<usize>],
        active_height: u16,
        commit_capacity_policy: CommitCapacityPolicy,
    ) -> ComputedLayout {
        let base_cfg = LayoutConfig {
            active_height,
            panel_height: state.bottom_panel.desired_height(root.width.max(1)),
            commit_capacity_policy,
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

        let active_height = clamp_active_height(
            active_height,
            root.height,
            base_cfg.panel_height,
            input_height,
            base_cfg.info_height,
        );
        let pass2_cfg = LayoutConfig {
            active_height,
            input_height,
            ..base_cfg.clone()
        };
        let pass2 = pass2_cfg.compute(root);
        let chat_height = renderer.desired_height(chat_view, pass2.chat_area);

        let final_cfg = LayoutConfig {
            chat_height,
            input_height,
            ..pass2_cfg
        };

        final_cfg.compute(root)
    }
}

fn clamp_active_height(
    desired: u16,
    root_height: u16,
    panel_height: u16,
    input_height: u16,
    info_height: u16,
) -> u16 {
    if desired == 0 {
        return 0;
    }

    let max_height = root_height
        .saturating_sub(panel_height)
        .saturating_sub(input_height)
        .saturating_sub(info_height)
        .saturating_sub(MIN_CHAT_HEIGHT);
    if max_height == 0 {
        return 0;
    }

    desired.clamp(MIN_ACTIVE_HEIGHT.min(max_height), max_height)
}
