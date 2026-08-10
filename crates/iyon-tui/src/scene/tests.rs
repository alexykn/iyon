use super::*;
use crate::{
    component::{Component, ComponentRegistry, MountedComponents},
    geometry::Size,
    interaction::FocusState,
    presentation::{IntoView, View, layout::compile_view},
};

use super::{LayoutSynchronizer, layout_resolved_scene};

#[derive(Debug)]
struct Label {
    text: String,
}

impl Component for Label {
    fn view(&self) -> View {
        View::text(self.text.clone()).into_view()
    }
}

#[derive(Debug)]
struct LayoutAware {
    size: Option<Size>,
    calls: usize,
}

impl Component for LayoutAware {
    fn view(&self) -> View {
        View::text("layout").into_view()
    }

    fn capabilities(&self, cx: &mut crate::ComponentCx<'_, Self>) {
        cx.on_layout_changed(Self::layout_changed);
    }
}

impl LayoutAware {
    fn layout_changed(&mut self, size: Size) {
        self.size = Some(size);
        self.calls += 1;
    }
}

#[derive(Debug)]
struct HangingBody {
    size: Option<Size>,
    calls: usize,
}

impl Component for HangingBody {
    fn view(&self) -> View {
        View::text("A1 long content\nA2").into_view()
    }

    fn capabilities(&self, cx: &mut crate::ComponentCx<'_, Self>) {
        cx.on_layout_changed(Self::layout_changed);
    }
}

impl HangingBody {
    fn layout_changed(&mut self, size: Size) {
        self.size = Some(size);
        self.calls += 1;
    }
}

#[derive(Debug)]
struct FocusableLabel;

impl Component for FocusableLabel {
    fn view(&self) -> View {
        View::text("focus").into_view()
    }

    fn capabilities(&self, cx: &mut crate::ComponentCx<'_, Self>) {
        cx.focusable();
    }
}

#[derive(Debug)]
struct FocusableMultiline;

impl Component for FocusableMultiline {
    fn view(&self) -> View {
        View::text("top\nbottom").into_view()
    }

    fn capabilities(&self, cx: &mut crate::ComponentCx<'_, Self>) {
        cx.focusable();
    }
}

#[derive(Debug)]
struct ModalHost {
    child: crate::component::ComponentHandle<FocusableLabel>,
}

impl Component for ModalHost {
    fn view(&self) -> View {
        View::component(self.child)
    }

    fn capabilities(&self, cx: &mut crate::ComponentCx<'_, Self>) {
        cx.modal_scope();
    }
}

#[derive(Debug)]
struct Parent {
    child: crate::component::ComponentHandle<Label>,
}

impl Component for Parent {
    fn view(&self) -> View {
        View::vertical(|column| {
            column.child("parent");
            column.child(View::component(self.child));
        })
    }
}

#[derive(Debug)]
struct CycleNode {
    target: Option<crate::component::ComponentId>,
}

impl Component for CycleNode {
    fn view(&self) -> View {
        self.target
            .map(|id| View {
                component: None,
                width: crate::presentation::WidthRule::Fit,
                height: crate::presentation::ir::HeightRule::Fit,
                decoration: Default::default(),
                kind: crate::presentation::ir::ViewKind::ComponentSlot(
                    crate::presentation::ir::ComponentSlotNode { id },
                ),
            })
            .unwrap_or_else(|| View::text("cycle").into_view())
    }
}

#[test]
fn static_scene_resolves_identically_with_no_mounts() {
    let registry = ComponentRegistry::new();
    let original = View::vertical(|column| {
        column.child("a");
        column.child(View::spacer(1));
    });

    let resolved = resolve_scene(&original, &registry).unwrap();
    assert_eq!(resolved.view, original);
    assert!(resolved.mounts.is_empty());
}

#[test]
fn hanging_body_component_is_mounted_once_and_owns_one_body_geometry() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(HangingBody {
        size: None,
        calls: 0,
    });
    let view = View::hanging(
        View::text("• ").no_wrap(),
        View::text("  ").no_wrap(),
        View::component(handle),
    )
    .fill_width();
    let resolved = resolve_scene(&view, &registry).unwrap();
    let rows = compile_view(&resolved.view, 10).rows;

    assert_eq!(
        resolved
            .mounts
            .nodes
            .iter()
            .filter(|node| node.id == handle.id())
            .count(),
        1
    );
    assert_eq!(
        rows.iter().map(|row| row.plain_text()).collect::<Vec<_>>(),
        ["• A1 long ", "  content", "  A2"]
    );

    let layout = layout_resolved_scene(&resolved, Size::new(10, 3));
    let geometry = layout.components.entries.get(&handle.id()).unwrap();
    assert_eq!(geometry.content, crate::geometry::Rect::new(2, 0, 8, 3));
    assert_eq!(layout.components.entries.len(), 1);
}

#[test]
fn hanging_body_component_reflows_without_mount_duplication() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(HangingBody {
        size: None,
        calls: 0,
    });
    let view = View::hanging(
        View::text("• ").no_wrap(),
        View::text("  ").no_wrap(),
        View::component(handle),
    )
    .fill_width();
    let resolved = resolve_scene(&view, &registry).unwrap();
    let mut synchronizer = LayoutSynchronizer::default();

    let wide = layout_resolved_scene(&resolved, Size::new(20, 10));
    let wide_rows = compile_view(&resolved.view, 20).rows;
    assert_eq!(wide_rows.len(), 2);
    assert_eq!(
        synchronizer.synchronize(
            &resolved.mounts,
            &resolved.capabilities,
            &wide.components,
            &mut registry,
        ),
        super::LayoutSync::Dirty
    );
    let wide_size = wide.components.entries[&handle.id()].content.size();

    let narrow = layout_resolved_scene(&resolved, Size::new(8, 10));
    let narrow_rows = compile_view(&resolved.view, 8).rows;
    assert_eq!(narrow_rows.len(), 5);
    assert_eq!(
        synchronizer.synchronize(
            &resolved.mounts,
            &resolved.capabilities,
            &narrow.components,
            &mut registry,
        ),
        super::LayoutSync::Dirty
    );
    let narrow_size = narrow.components.entries[&handle.id()].content.size();

    assert_eq!(
        resolved
            .mounts
            .nodes
            .iter()
            .filter(|node| node.id == handle.id())
            .count(),
        1
    );
    assert_eq!(wide_size.width, 18);
    assert_eq!(narrow_size.width, 6);
    assert!(narrow_size.height > wide_size.height);
    assert_eq!(
        registry.with(handle, |body| body.size),
        Some(Some(narrow_size))
    );
    assert_eq!(registry.with(handle, |body| body.calls), Some(2));
}

#[test]
fn hanging_prefix_component_is_mounted_once_when_body_wraps() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "• ".into()
    });
    let view = View::hanging(
        View::component(handle),
        View::text("  ").no_wrap(),
        View::text("body that wraps").fill_width(),
    )
    .fill_width();
    let resolved = resolve_scene(&view, &registry).unwrap();
    let layout = layout_resolved_scene(&resolved, Size::new(8, 4));

    assert_eq!(
        resolved
            .mounts
            .nodes
            .iter()
            .filter(|node| node.id == handle.id())
            .count(),
        1
    );
    assert_eq!(layout.components.entries.len(), 1);
    assert!(compile_view(&resolved.view, 8).rows.len() > 1);
}

#[test]
#[should_panic(expected = "hanging continuation_prefix cannot contain component identity")]
fn hanging_continuation_component_is_rejected_before_resolution() {
    let mut registry = ComponentRegistry::new();
    let repeated = registry.register(Label { text: "  ".into() });
    let _ = View::hanging(
        View::text("• "),
        View::component(repeated),
        View::text("body long enough to wrap"),
    );
}

#[test]
fn one_slot_becomes_an_owned_component_root() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "hello".into(),
    });
    let resolved = resolve_scene(&View::component(handle), &registry).unwrap();

    assert_eq!(resolved.mounts.nodes.len(), 1);
    assert_eq!(resolved.mounts.nodes[0].id, handle.id());
    assert_eq!(resolved.mounts.nodes[0].parent, None);
    assert_eq!(resolved.view.component, Some(handle.id()));
    assert_eq!(count_slots(&resolved.view), 0);
}

#[test]
fn nested_child_is_reread_from_registry_on_each_resolution() {
    let mut registry = ComponentRegistry::new();
    let child = registry.register(Label { text: "old".into() });
    let parent = registry.register(Parent { child });

    let first = resolve_scene(&View::component(parent), &registry).unwrap();
    assert_eq!(
        first
            .mounts
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        vec![parent.id(), child.id()]
    );
    assert_eq!(first.mounts.nodes[1].parent, Some(parent.id()));
    assert!(first.view.view_plain_text().contains("old"));

    registry.with_mut(child, |label| label.text = "new".into());
    let second = resolve_scene(&View::component(parent), &registry).unwrap();
    assert!(second.view.view_plain_text().contains("new"));
    assert_eq!(second.mounts.nodes[1].revision.value(), 1);
}

#[test]
fn slot_properties_become_the_component_ownership_shell() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "hello".into(),
    });
    let slot = View::component(handle).padding(1).fill_width();
    let resolved = resolve_scene(&slot, &registry).unwrap().view;

    assert_eq!(resolved.component, Some(handle.id()));
    assert_eq!(resolved.width, crate::presentation::WidthRule::Fill);
    assert_eq!(
        resolved.decoration.padding,
        crate::presentation::Insets::all(1)
    );
    assert_eq!(count_slots(&resolved), 0);
    assert_eq!(compile_view(&resolved, 20).rows[1].plain_text(), " hello");
}

#[test]
fn explicit_outer_structure_stays_outside_component_ownership() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "hello".into(),
    });
    let resolved = resolve_scene(&View::component(handle).container().padding(1), &registry)
        .unwrap()
        .view;

    assert_eq!(resolved.component, None);
    let crate::presentation::ir::ViewKind::Container(container) = &resolved.kind else {
        panic!("expected outer container")
    };
    assert_eq!(container.child.component, Some(handle.id()));
}

#[test]
fn duplicate_slots_are_rejected_without_returning_a_scene() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label { text: "x".into() });
    let view = View::vertical(|column| {
        column.child(View::component(handle));
        column.child(View::component(handle));
    });

    assert_eq!(
        resolve_scene(&view, &registry),
        Err(ResolveError::DuplicateComponent { id: handle.id() })
    );
}

#[test]
fn failed_resolution_does_not_mutate_previous_mount_state() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "valid".into(),
    });
    let valid = resolve_scene(&View::component(handle), &registry).unwrap();
    let mut mounted = MountedComponents::default();
    mounted.reconcile(valid.mounts.clone());
    let previous = mounted.current().clone();

    let invalid = View::vertical(|column| {
        column.child(View::component(handle));
        column.child(View::component(handle));
    });
    assert!(matches!(
        resolve_scene(&invalid, &registry),
        Err(ResolveError::DuplicateComponent { .. })
    ));
    assert_eq!(mounted.current(), &previous);
}

#[test]
fn missing_slots_are_rejected() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label { text: "x".into() });
    registry.remove(handle);

    assert_eq!(
        resolve_scene(&View::component(handle), &registry),
        Err(ResolveError::MissingComponent { id: handle.id() })
    );
}

#[test]
fn self_and_multi_component_cycles_report_paths() {
    let mut registry = ComponentRegistry::new();
    let self_cycle = registry.register(CycleNode { target: None });
    registry.with_mut(self_cycle, |node| node.target = Some(self_cycle.id()));
    assert_eq!(
        resolve_scene(&View::component(self_cycle), &registry),
        Err(ResolveError::ComponentCycle {
            path: vec![self_cycle.id(), self_cycle.id()]
        })
    );

    let a = registry.register(CycleNode { target: None });
    let b = registry.register(CycleNode { target: None });
    let c = registry.register(CycleNode { target: None });
    registry.with_mut(a, |node| node.target = Some(b.id()));
    registry.with_mut(b, |node| node.target = Some(c.id()));
    registry.with_mut(c, |node| node.target = Some(a.id()));
    assert_eq!(
        resolve_scene(&View::component(a), &registry),
        Err(ResolveError::ComponentCycle {
            path: vec![a.id(), b.id(), c.id(), a.id()]
        })
    );
}

#[test]
fn wrong_registry_handles_cannot_alias_same_typed_components() {
    let mut first = ComponentRegistry::new();
    let handle = first.register(Label {
        text: "first".into(),
    });
    let second = ComponentRegistry::new();
    assert_eq!(
        resolve_scene(&View::component(handle), &second),
        Err(ResolveError::MissingComponent { id: handle.id() })
    );
}

#[test]
fn snapshot_metadata_does_not_create_a_live_mount() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "snapshot".into(),
    });
    let snapshot = registry.render(handle).unwrap();
    let resolved = resolve_scene(&snapshot, &registry).unwrap();

    assert_eq!(resolved.view, snapshot);
    assert!(resolved.mounts.is_empty());
}

#[test]
fn layout_size_delivery_is_initial_and_changes_only_when_size_changes() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(LayoutAware {
        size: None,
        calls: 0,
    });
    let resolved = resolve_scene(
        &View::component(handle).fill_width().fill_height(),
        &registry,
    )
    .unwrap();
    let layout = layout_resolved_scene(&resolved, Size::new(20, 4));
    let mut synchronizer = LayoutSynchronizer::default();
    assert_eq!(
        synchronizer.synchronize(
            &resolved.mounts,
            &resolved.capabilities,
            &layout.components,
            &mut registry,
        ),
        super::LayoutSync::Dirty
    );
    assert_eq!(registry.with(handle, |component| component.calls), Some(1));
    assert_eq!(
        synchronizer.synchronize(
            &resolved.mounts,
            &resolved.capabilities,
            &layout.components,
            &mut registry,
        ),
        super::LayoutSync::Stable
    );
    assert_eq!(registry.with(handle, |component| component.calls), Some(1));

    let resized = layout_resolved_scene(&resolved, Size::new(20, 5));
    assert_eq!(
        synchronizer.synchronize(
            &resolved.mounts,
            &resolved.capabilities,
            &resized.components,
            &mut registry,
        ),
        super::LayoutSync::Dirty
    );
    assert_eq!(registry.with(handle, |component| component.calls), Some(2));
}

#[test]
fn component_geometry_distinguishes_mounting_from_visibility() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "hidden".into(),
    });
    let view = View::component(handle)
        .padding(1)
        .clamp_rows(0, crate::presentation::OverflowIndicator::None);
    let resolved = resolve_scene(&view, &registry).unwrap();
    let layout = layout_resolved_scene(&resolved, Size::new(20, 4));
    let geometry = layout.components.entries.get(&handle.id()).unwrap();
    assert_eq!(geometry.visible, None);
    assert_eq!(resolved.mounts.nodes.len(), 1);
}

#[test]
fn focus_excludes_fully_clipped_components() {
    let mut registry = ComponentRegistry::new();
    let hidden = registry.register(FocusableLabel);
    let visible = registry.register(FocusableLabel);
    let view = View::vertical(|column| {
        column.child(
            View::component(hidden).clamp_rows(0, crate::presentation::OverflowIndicator::None),
        );
        column.child(View::component(visible).fill_height());
    })
    .fill_width()
    .fill_height();
    let resolved = resolve_scene(&view, &registry).unwrap();
    let layout = layout_resolved_scene(&resolved, Size::new(20, 4));
    let mut focus = FocusState::default();
    focus.reconcile_with_geometry(
        &resolved.mounts,
        &resolved.capabilities,
        Some(&layout.components),
        &mut registry,
    );
    assert_eq!(focus.focused(), Some(visible.id()));
}

#[test]
fn partially_clipped_focusable_remains_eligible() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(FocusableMultiline);
    let view = View::component(handle).clamp_rows(1, crate::presentation::OverflowIndicator::None);
    let resolved = resolve_scene(&view, &registry).unwrap();
    let layout = layout_resolved_scene(&resolved, Size::new(20, 4));
    assert!(
        layout
            .components
            .entries
            .get(&handle.id())
            .and_then(|geometry| geometry.visible)
            .is_some()
    );

    let mut focus = FocusState::default();
    focus.reconcile_with_geometry(
        &resolved.mounts,
        &resolved.capabilities,
        Some(&layout.components),
        &mut registry,
    );
    assert_eq!(focus.focused(), Some(handle.id()));
}

#[test]
fn invisible_focus_repairs_without_stealing_focus_on_return() {
    let mut registry = ComponentRegistry::new();
    let first = registry.register(FocusableLabel);
    let second = registry.register(FocusableLabel);
    let resolved = resolve_scene(
        &View::vertical(|column| {
            column.child(View::component(first));
            column.child(View::component(second));
        }),
        &registry,
    )
    .unwrap();
    let layout = layout_resolved_scene(&resolved, Size::new(20, 4));
    let mut focus = FocusState::default();
    focus.reconcile_with_geometry(
        &resolved.mounts,
        &resolved.capabilities,
        Some(&layout.components),
        &mut registry,
    );
    assert_eq!(focus.focused(), Some(first.id()));

    let mut hidden = layout.components.clone();
    hidden.entries.get_mut(&first.id()).unwrap().visible = None;
    focus.reconcile_with_geometry(
        &resolved.mounts,
        &resolved.capabilities,
        Some(&hidden),
        &mut registry,
    );
    assert_eq!(focus.focused(), Some(second.id()));
    focus.reconcile_with_geometry(
        &resolved.mounts,
        &resolved.capabilities,
        Some(&layout.components),
        &mut registry,
    );
    assert_eq!(focus.focused(), Some(second.id()));
}

#[test]
fn invisible_active_modal_does_not_expose_background_focus() {
    let mut registry = ComponentRegistry::new();
    let modal_child = registry.register(FocusableLabel);
    let modal = registry.register(ModalHost { child: modal_child });
    let background = registry.register(FocusableLabel);
    let resolved = resolve_scene(
        &View::vertical(|column| {
            column.child(View::component(modal));
            column.child(View::component(background));
        }),
        &registry,
    )
    .unwrap();
    let layout = layout_resolved_scene(&resolved, Size::new(20, 4));
    let mut hidden_modal = layout.components.clone();
    hidden_modal
        .entries
        .get_mut(&modal_child.id())
        .unwrap()
        .visible = None;

    let mut focus = FocusState::default();
    focus.reconcile_with_geometry(
        &resolved.mounts,
        &resolved.capabilities,
        Some(&hidden_modal),
        &mut registry,
    );
    assert_eq!(focus.active_modal(), Some(modal.id()));
    assert_eq!(focus.focused(), None);
}

#[test]
fn clipped_component_remains_semantically_mounted() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Label {
        text: "hidden".into(),
    });
    let view = View::component(handle).clamp_rows(0, crate::presentation::OverflowIndicator::None);
    let resolved = resolve_scene(&view, &registry).unwrap();

    assert_eq!(resolved.mounts.nodes[0].id, handle.id());
    assert!(compile_view(&resolved.view, 20).rows.is_empty());
}

fn count_slots(view: &View) -> usize {
    match &view.kind {
        crate::presentation::ir::ViewKind::ComponentSlot(_) => 1,
        crate::presentation::ir::ViewKind::Column(column) => column
            .children
            .iter()
            .map(|child| count_slots(&child.view))
            .sum(),
        crate::presentation::ir::ViewKind::Row(row) => row
            .children
            .iter()
            .map(|child| count_slots(&child.view))
            .sum(),
        crate::presentation::ir::ViewKind::Container(container) => count_slots(&container.child),
        crate::presentation::ir::ViewKind::Hanging(hanging) => {
            count_slots(&hanging.prefix)
                + count_slots(&hanging.continuation_prefix)
                + count_slots(&hanging.body)
        }
        crate::presentation::ir::ViewKind::ClampRows(clamp) => count_slots(&clamp.child),
        crate::presentation::ir::ViewKind::RowViewport(viewport) => count_slots(&viewport.child),
        crate::presentation::ir::ViewKind::Text(_)
        | crate::presentation::ir::ViewKind::Spacer { .. } => 0,
    }
}

trait PlainText {
    fn view_plain_text(&self) -> String;
}

impl PlainText for crate::presentation::ir::ColumnChild {
    fn view_plain_text(&self) -> String {
        self.view.view_plain_text()
    }
}

impl PlainText for View {
    fn view_plain_text(&self) -> String {
        match &self.kind {
            crate::presentation::ir::ViewKind::Text(text) => {
                text.spans.iter().map(|span| span.text.as_str()).collect()
            }
            crate::presentation::ir::ViewKind::Column(column) => column
                .children
                .iter()
                .map(PlainText::view_plain_text)
                .collect::<Vec<_>>()
                .join(""),
            crate::presentation::ir::ViewKind::Container(container) => {
                container.child.view_plain_text()
            }
            crate::presentation::ir::ViewKind::Hanging(hanging) => format!(
                "{}{}{}",
                hanging.prefix.view_plain_text(),
                hanging.continuation_prefix.view_plain_text(),
                hanging.body.view_plain_text()
            ),
            crate::presentation::ir::ViewKind::ComponentSlot(_) => String::new(),
            crate::presentation::ir::ViewKind::Row(row) => row
                .children
                .iter()
                .map(|child| child.view.view_plain_text())
                .collect(),
            crate::presentation::ir::ViewKind::ClampRows(clamp) => clamp.child.view_plain_text(),
            crate::presentation::ir::ViewKind::RowViewport(viewport) => {
                viewport.child.view_plain_text()
            }
            crate::presentation::ir::ViewKind::Spacer { .. } => String::new(),
        }
    }
}
