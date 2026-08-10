//! Width-dependent semantic projection of History into one vertical View.

use crate::{
    component::ComponentRegistry,
    geometry::{LayoutConstraints, Size},
    physical::PhysicalRow,
    presentation::{
        Insets, View,
        layout::{LayoutEngine, ManualLayoutEngine},
    },
    scene::{ResolveError, ResolveSession, ResolvedScene},
    stream::StreamRowIndex,
};

use super::{FlowBoundary, History, HistoryUnitContent, native::frontier::SpacingTransferState};
use crate::stream::FrozenPhysicalRows;

#[derive(Debug, PartialEq)]
pub(crate) struct HistoryProjection {
    pub(crate) scene: ResolvedScene,
    pub(crate) frozen_overlay: Option<HistoryPhysicalOverlay>,
}

struct SemanticProjection {
    view: View,
    frozen_overlay: Option<HistoryPhysicalOverlay>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HistoryPhysicalOverlay {
    pub(crate) row: u16,
    pub(crate) rows: Vec<PhysicalRow>,
}

struct UnitPlan {
    boundary: FlowBoundary,
    height: Option<usize>,
    content: PlannedContent,
}

enum PlannedContent {
    Static,
    Frozen(FrozenPhysicalRows),
    Live(View),
    Stream {
        index: Option<StreamRowIndex>,
        start: crate::stream::StreamOffset,
        prefix: Option<FrozenPhysicalRows>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FlowItem {
    TopPadding,
    Unit(usize),
    Gap(usize),
    BottomPadding,
}

#[derive(Clone, Copy)]
struct Selected {
    offset: usize,
    height: usize,
}

pub(crate) fn project(
    history: &History,
    registry: &ComponentRegistry,
    size: Size,
) -> Result<HistoryProjection, ResolveError> {
    let mut session = ResolveSession::new(registry);
    let projection = project_view(history, size, &mut session)?;
    let scene = session.finish(projection.view);
    Ok(HistoryProjection {
        scene,
        frozen_overlay: projection.frozen_overlay,
    })
}

fn project_view(
    history: &History,
    size: Size,
    session: &mut ResolveSession<'_>,
) -> Result<SemanticProjection, ResolveError> {
    let layout = history.layout();
    let content_width = size
        .width
        .saturating_sub(layout.padding.left.saturating_add(layout.padding.right));
    let units = history.units().collect::<Vec<_>>();
    let mut plans = units
        .iter()
        .map(|unit| match &unit.content {
            HistoryUnitContent::Static(_) => Ok(UnitPlan {
                boundary: unit.boundary,
                height: None,
                content: PlannedContent::Static,
            }),
            HistoryUnitContent::Live(view) => {
                let resolved = session.resolve_root(&view)?;
                Ok(UnitPlan {
                    boundary: unit.boundary,
                    height: None,
                    content: PlannedContent::Live(resolved),
                })
            }
            HistoryUnitContent::Stream(_) => {
                let (start, prefix) = stream_projection_state(history, unit.id);
                Ok(UnitPlan {
                    boundary: unit.boundary,
                    height: None,
                    content: PlannedContent::Stream {
                        index: None,
                        start,
                        prefix,
                    },
                })
            }
        })
        .collect::<Result<Vec<_>, ResolveError>>()?;

    let mut selected_units = vec![None; plans.len()];
    let mut selected_items = Vec::new();
    let mut remaining = usize::from(size.height);
    let top_padding = resident_top_padding(history);

    if let Some(rows) = frozen_static_rows(history) {
        if let Some(plan) = plans.first_mut() {
            plan.height = Some(rows.len());
            plan.content = PlannedContent::Frozen(rows);
        }
    }

    // History follows its semantic end. Walk backwards until the viewport is
    // full; the first selected item may be a partial unit, gap, or padding.
    take_selected(
        FlowItem::BottomPadding,
        usize::from(layout.padding.bottom),
        &mut remaining,
        &mut selected_units,
        &mut selected_items,
    );
    for index in (0..plans.len()).rev() {
        if remaining == 0 {
            break;
        }
        let height = ensure_height(&mut plans[index], units[index], content_width);
        take_selected(
            FlowItem::Unit(index),
            height,
            &mut remaining,
            &mut selected_units,
            &mut selected_items,
        );
        if remaining == 0 {
            break;
        }
        if has_predecessor_gap(history, index, &plans[index]) {
            take_selected(
                FlowItem::Gap(index),
                resident_gap(history, index, &plans[index]),
                &mut remaining,
                &mut selected_units,
                &mut selected_items,
            );
        }
    }
    if remaining > 0 {
        take_selected(
            FlowItem::TopPadding,
            top_padding,
            &mut remaining,
            &mut selected_units,
            &mut selected_items,
        );
    }

    let items = flow_items(history, &plans);
    let mut children = Vec::new();
    let slack = remaining;
    push_spacer(&mut children, slack);
    let mut rendered_row = slack;
    let mut frozen_overlay = None;
    for item in items {
        match item {
            FlowItem::Unit(index) => {
                if let Some(selected) = selected_units[index] {
                    if let Some((visible, row_offset)) =
                        frozen_visible_rows(&plans[index], selected)
                    {
                        if !visible.is_empty() {
                            frozen_overlay = Some(HistoryPhysicalOverlay {
                                row: rendered_row
                                    .saturating_add(row_offset)
                                    .min(usize::from(u16::MAX))
                                    as u16,
                                rows: visible,
                            });
                        }
                    }
                    children.push(unit_view(&plans[index], units[index], selected));
                    rendered_row = rendered_row.saturating_add(selected.height);
                } else if let PlannedContent::Live(view) = &plans[index].content {
                    children.push(View::row_viewport_with_height(view.clone(), 0, Some(0)));
                }
            }
            FlowItem::TopPadding | FlowItem::Gap(_) | FlowItem::BottomPadding => {
                if let Some((_, selected)) = selected_items
                    .iter()
                    .find(|(candidate, _)| *candidate == item)
                {
                    push_spacer(&mut children, selected.height);
                    rendered_row = rendered_row.saturating_add(selected.height);
                }
            }
        }
    }

    let mut root = View::vertical(|column| {
        column.children(children);
    });
    root = root.fill_width().fill_height().padding(Insets::new(
        0,
        layout.padding.right,
        0,
        layout.padding.left,
    ));
    Ok(SemanticProjection {
        view: root,
        frozen_overlay,
    })
}

fn frozen_visible_rows(plan: &UnitPlan, selected: Selected) -> Option<(Vec<PhysicalRow>, usize)> {
    let (rows, prefix_len) = match &plan.content {
        PlannedContent::Frozen(rows) => (rows, rows.as_slice().len()),
        PlannedContent::Stream {
            prefix: Some(rows), ..
        } => (rows, rows.as_slice().len()),
        _ => return None,
    };
    let end = selected.offset.saturating_add(selected.height);
    let visible_end = end.min(prefix_len);
    let visible_start = selected.offset.min(visible_end);
    (visible_start < visible_end).then(|| {
        (
            rows.as_slice()[visible_start..visible_end].to_vec(),
            visible_start.saturating_sub(selected.offset),
        )
    })
}

fn stream_projection_state(
    history: &History,
    unit: super::HistoryUnitId,
) -> (crate::stream::StreamOffset, Option<FrozenPhysicalRows>) {
    let semantic_base = history
        .units
        .iter()
        .find(|candidate| candidate.id == unit)
        .and_then(|candidate| match &candidate.content {
            HistoryUnitContent::Stream(stream) => Some(stream.semantic_base()),
            _ => None,
        })
        .unwrap_or(crate::stream::StreamOffset::ZERO);
    let Some(state) = history
        .native
        .stream
        .as_ref()
        .filter(|state| state.unit == unit)
    else {
        return (semantic_base, None);
    };
    match &state.partial {
        Some(crate::stream::StreamPartialCommit::FrozenAtomic {
            source_end,
            rows,
            committed_rows,
            ..
        }) => (
            *source_end,
            Some(FrozenPhysicalRows::new(
                rows.as_slice()[*committed_rows..].to_vec(),
            )),
        ),
        None => (state.committed_through, None),
    }
}

fn resident_top_padding(history: &History) -> usize {
    match &history.native.top_padding {
        SpacingTransferState::Semantic => usize::from(history.layout().padding.top),
        SpacingTransferState::Frozen(rows) => rows.as_slice().len(),
        SpacingTransferState::Native => 0,
    }
}

fn frozen_static_rows(history: &History) -> Option<FrozenPhysicalRows> {
    history
        .native
        .frozen_static
        .as_ref()
        .map(|frozen| frozen.rows.clone())
}

fn has_predecessor_gap(history: &History, index: usize, plan: &UnitPlan) -> bool {
    if !matches!(plan.boundary, FlowBoundary::Default) {
        return false;
    }
    if index > 0 {
        return true;
    }
    let Some(_) = history.native.last_native_unit else {
        return false;
    };
    !matches!(
        history.native.leading_gap,
        Some(SpacingTransferState::Native)
    )
}

fn resident_gap(history: &History, index: usize, _plan: &UnitPlan) -> usize {
    if index > 0 {
        return usize::from(history.layout().gap);
    }
    match history.native.leading_gap.as_ref() {
        Some(SpacingTransferState::Frozen(rows)) => rows.as_slice().len(),
        Some(SpacingTransferState::Native) => 0,
        Some(SpacingTransferState::Semantic) | None => usize::from(history.layout().gap),
    }
}

fn flow_items(history: &History, plans: &[UnitPlan]) -> Vec<FlowItem> {
    let mut items = vec![FlowItem::TopPadding];
    for (index, plan) in plans.iter().enumerate() {
        if has_predecessor_gap(history, index, plan) {
            items.push(FlowItem::Gap(index));
        }
        items.push(FlowItem::Unit(index));
    }
    items.push(FlowItem::BottomPadding);
    items
}

fn ensure_height(plan: &mut UnitPlan, unit: &super::HistoryUnit, width: u16) -> usize {
    if let Some(height) = plan.height {
        return height;
    }
    let height = match (&mut plan.content, &unit.content) {
        (PlannedContent::Static, HistoryUnitContent::Static(view)) => view_height(view, width),
        (PlannedContent::Frozen(rows), _) => rows.len(),
        (PlannedContent::Live(view), _) => view_height(view, width),
        (
            PlannedContent::Stream {
                index,
                start,
                prefix,
            },
            HistoryUnitContent::Stream(stream),
        ) => {
            let prepared = index.get_or_insert_with(|| stream.prepare_from(*start, width));
            prefix.as_ref().map_or(0, |rows| rows.as_slice().len()) + prepared.anchors.len()
        }
        _ => unreachable!("History projection plan does not match its unit"),
    };
    plan.height = Some(height);
    height
}

fn take_selected(
    item: FlowItem,
    height: usize,
    remaining: &mut usize,
    selected_units: &mut [Option<Selected>],
    selected_items: &mut Vec<(FlowItem, Selected)>,
) {
    if height == 0 || *remaining == 0 {
        return;
    }
    let visible = (*remaining).min(height);
    let selected = Selected {
        offset: height.saturating_sub(visible),
        height: visible,
    };
    match item {
        FlowItem::Unit(index) => selected_units[index] = Some(selected),
        _ => selected_items.push((item, selected)),
    }
    *remaining = (*remaining).saturating_sub(visible);
}

fn unit_view(plan: &UnitPlan, unit: &super::HistoryUnit, selected: Selected) -> View {
    match &plan.content {
        PlannedContent::Static => {
            let HistoryUnitContent::Static(view) = &unit.content else {
                unreachable!("static plan must match static unit")
            };
            View::row_viewport_with_height(
                view.clone(),
                selected.offset.min(usize::from(u16::MAX)) as u16,
                Some(selected.height.min(usize::from(u16::MAX)) as u16),
            )
        }
        PlannedContent::Frozen(_) => {
            View::spacer(selected.height.min(usize::from(u16::MAX)) as u16)
        }
        PlannedContent::Live(view) => View::row_viewport_with_height(
            view.clone(),
            selected.offset.min(usize::from(u16::MAX)) as u16,
            Some(selected.height.min(usize::from(u16::MAX)) as u16),
        ),
        PlannedContent::Stream { index, prefix, .. } => {
            let HistoryUnitContent::Stream(stream) = &unit.content else {
                unreachable!("stream plan must match stream unit")
            };
            let index = index
                .as_ref()
                .expect("selected stream must have a prepared index");
            let prefix_len = prefix.as_ref().map_or(0, |rows| rows.as_slice().len());
            let end = selected.offset.saturating_add(selected.height);
            let prefix_end = end.min(prefix_len);
            let prefix_start = selected.offset.min(prefix_end);
            let prefix_height = prefix_end.saturating_sub(prefix_start);
            let semantic_offset = selected.offset.saturating_sub(prefix_len);
            let semantic_height = selected.height.saturating_sub(prefix_height);
            let mut children = Vec::new();
            if prefix_height > 0 {
                children.push(View::spacer(prefix_height.min(usize::from(u16::MAX)) as u16));
            }
            if semantic_height > 0 {
                children.push(stream.window_view(
                    index,
                    semantic_offset,
                    semantic_height.min(usize::from(u16::MAX)) as u16,
                ));
            }
            View::column(children, 0)
        }
    }
}

fn push_spacer(children: &mut Vec<View>, rows: usize) {
    if rows == 0 {
        return;
    }
    children.push(View::spacer(rows.min(usize::from(u16::MAX)) as u16));
}

fn view_height(view: &View, width: u16) -> usize {
    usize::from(
        ManualLayoutEngine
            .layout(view, LayoutConstraints::width_only(width))
            .size
            .height,
    )
}
