//! Width-dependent semantic projection of History into one vertical View.

use crate::{
    component::ComponentRegistry,
    geometry::{LayoutConstraints, Size},
    presentation::{
        Insets, View,
        layout::{LayoutEngine, ManualLayoutEngine},
    },
    scene::{ResolveError, ResolveSession, ResolvedScene},
    stream::StreamRowIndex,
};

use super::{FlowBoundary, History, HistoryUnitContent};

pub(crate) struct HistoryProjection {
    pub(crate) view: View,
}

struct UnitPlan {
    boundary: FlowBoundary,
    height: Option<usize>,
    content: PlannedContent,
}

enum PlannedContent {
    Static,
    Live(View),
    Stream(StreamRowIndex),
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
) -> Result<ResolvedScene, ResolveError> {
    let mut session = ResolveSession::new(registry);
    let projection = project_view(history, size, &mut session)?;
    Ok(session.finish(projection.view))
}

fn project_view(
    history: &History,
    size: Size,
    session: &mut ResolveSession<'_>,
) -> Result<HistoryProjection, ResolveError> {
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
            HistoryUnitContent::Stream(stream) => {
                let index = stream.prepare(content_width);
                Ok(UnitPlan {
                    boundary: unit.boundary,
                    height: Some(index.anchors.len()),
                    content: PlannedContent::Stream(index),
                })
            }
        })
        .collect::<Result<Vec<_>, ResolveError>>()?;

    let mut selected_units = vec![None; plans.len()];
    let mut selected_items = Vec::new();
    let mut remaining = usize::from(size.height);

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
        if index > 0 && matches!(plans[index].boundary, FlowBoundary::Default) {
            take_selected(
                FlowItem::Gap(index),
                usize::from(layout.gap),
                &mut remaining,
                &mut selected_units,
                &mut selected_items,
            );
        }
    }
    if remaining > 0 {
        take_selected(
            FlowItem::TopPadding,
            usize::from(layout.padding.top),
            &mut remaining,
            &mut selected_units,
            &mut selected_items,
        );
    }

    let items = flow_items(&plans);
    let mut children = Vec::new();
    let slack = remaining;
    push_spacer(&mut children, slack);
    for item in items {
        match item {
            FlowItem::Unit(index) => {
                if let Some(selected) = selected_units[index] {
                    children.push(unit_view(&plans[index], units[index], selected));
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
    Ok(HistoryProjection { view: root })
}

fn flow_items(plans: &[UnitPlan]) -> Vec<FlowItem> {
    let mut items = vec![FlowItem::TopPadding];
    for (index, plan) in plans.iter().enumerate() {
        if index > 0 && matches!(plan.boundary, FlowBoundary::Default) {
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
    let height = match (&plan.content, &unit.content) {
        (PlannedContent::Static, HistoryUnitContent::Static(view)) => view_height(view, width),
        (PlannedContent::Live(view), _) => view_height(view, width),
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
        PlannedContent::Live(view) => View::row_viewport_with_height(
            view.clone(),
            selected.offset.min(usize::from(u16::MAX)) as u16,
            Some(selected.height.min(usize::from(u16::MAX)) as u16),
        ),
        PlannedContent::Stream(index) => {
            let HistoryUnitContent::Stream(stream) = &unit.content else {
                unreachable!("stream plan must match stream unit")
            };
            let view = stream.window_view(
                index,
                selected.offset,
                selected.height.min(usize::from(u16::MAX)) as u16,
            );
            View::row_viewport_with_height(
                view,
                0,
                Some(selected.height.min(usize::from(u16::MAX)) as u16),
            )
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
