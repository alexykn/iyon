use super::super::*;
use crate::{View, component::ComponentId, presentation::IntoView};

fn live_view(label: &str) -> View {
    View::text(label)
        .into_view()
        .with_component(ComponentId::allocate())
}

fn unit_ids(history: &History) -> Vec<HistoryUnitId> {
    history.units().map(|unit| unit.id).collect()
}

#[test]
fn component_free_views_are_static_even_when_nested() {
    let mut history = History::new();
    let id = history
        .push(View::vertical(|column| {
            column.child(
                View::text("text")
                    .container()
                    .clamp_rows(3, crate::OverflowIndicator::None),
            );
            column.child(View::horizontal(|row| {
                row.child("styled");
                row.child(View::text("nested").container());
            }));
        }))
        .unwrap();

    let unit = history.units().find(|unit| unit.id == id).unwrap();
    assert!(matches!(unit.content, HistoryUnitContent::Static(_)));
}

#[test]
fn multiple_live_units_remain_independent_and_ordered() {
    let mut history = History::new();
    let a = history.push("A").unwrap();
    let b = history.push(live_view("B")).unwrap();
    let c = history
        .push_with_boundary("C", FlowBoundary::AttachToPrevious)
        .unwrap();
    let d = history.push(live_view("D")).unwrap();
    let e = history.push("E").unwrap();

    assert_eq!(unit_ids(&history), vec![a, b, c, d, e]);
    assert_eq!(
        history
            .units()
            .map(|unit| unit.boundary)
            .collect::<Vec<_>>(),
        vec![
            FlowBoundary::Default,
            FlowBoundary::Default,
            FlowBoundary::AttachToPrevious,
            FlowBoundary::Default,
            FlowBoundary::Default,
        ]
    );
    assert!(matches!(
        &history.units().nth(1).unwrap().content,
        HistoryUnitContent::Live(_)
    ));
    assert!(matches!(
        &history.units().nth(3).unwrap().content,
        HistoryUnitContent::Live(_)
    ));
}

#[test]
fn freeze_preserves_id_position_and_boundary() {
    let mut history = History::new();
    let first = history.push("A").unwrap();
    let live = history
        .push_with_boundary(live_view("B"), FlowBoundary::AttachToPrevious)
        .unwrap();
    let last = history.push("C").unwrap();
    let before = history
        .units()
        .find(|unit| unit.id == live)
        .map(|unit| (unit.id, unit.boundary))
        .unwrap();

    history.freeze(live, "final B").unwrap();

    assert_eq!(unit_ids(&history), vec![first, live, last]);
    let frozen = history.units().nth(1).unwrap();
    assert_eq!((frozen.id, frozen.boundary), before);
    assert!(matches!(frozen.content, HistoryUnitContent::Static(_)));
}

#[test]
fn freeze_rejects_component_final_view_without_mutating_live_unit() {
    let mut history = History::new();
    let live = history
        .push_with_boundary(live_view("live"), FlowBoundary::AttachToPrevious)
        .unwrap();
    let before = history
        .units()
        .find(|unit| unit.id == live)
        .map(|unit| (unit.id, unit.boundary))
        .unwrap();

    assert_eq!(
        history.freeze(live, live_view("still live")),
        Err(HistoryError::FinalViewContainsComponent { unit: live })
    );
    let unchanged = history.units().find(|unit| unit.id == live).unwrap();
    assert_eq!((unchanged.id, unchanged.boundary), before);
    assert!(matches!(unchanged.content, HistoryUnitContent::Live(_)));
}

#[test]
fn first_unit_boundary_is_preserved() {
    let mut history = History::new();
    let id = history
        .push_with_boundary("first", FlowBoundary::AttachToPrevious)
        .unwrap();
    let unit = history.units().find(|unit| unit.id == id).unwrap();
    assert_eq!(unit.boundary, FlowBoundary::AttachToPrevious);
}
