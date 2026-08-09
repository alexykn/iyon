use super::*;
use crate::presentation::{
    Decoration, IntoView, OverflowIndicator, StyleSpec, View, layout::compile_view,
};

#[derive(Debug)]
struct Counter {
    value: u32,
}

impl Component for Counter {
    fn view(&self) -> View {
        View::text(self.value.to_string()).into_view()
    }
}

#[derive(Debug)]
struct SameVisual;

impl Component for SameVisual {
    fn view(&self) -> View {
        View::text("same").into_view()
    }
}

#[derive(Debug)]
struct Nested {
    child: View,
}

impl Component for Nested {
    fn view(&self) -> View {
        self.child.clone()
    }
}

#[test]
fn ids_are_unique_monotonic_and_never_reused_after_remove() {
    let mut registry = ComponentRegistry::new();
    let first = registry.register(Counter { value: 1 });
    let first_id = first.id().value();
    assert!(registry.remove(first).is_some());

    let second = registry.register(Counter { value: 2 });
    assert!(second.id().value() > first_id);
    assert_ne!(first.id(), second.id());
}

#[test]
fn typed_handles_resolve_only_their_own_state() {
    let mut registry = ComponentRegistry::new();
    let counter = registry.register(Counter { value: 7 });
    let wrong_type = ComponentHandle::<SameVisual>::from_id(counter.id());

    assert!(registry.contains(counter));
    assert!(!registry.contains(wrong_type));
    assert_eq!(registry.with(wrong_type, |_| ()), None);
    assert_eq!(registry.with_mut(wrong_type, |_| ()), None);
    assert!(registry.remove(wrong_type).is_none());
    assert_eq!(registry.with(counter, |value| value.value), Some(7));
}

#[test]
fn with_mut_changes_the_next_render_snapshot() {
    let mut registry = ComponentRegistry::new();
    let counter = registry.register(Counter { value: 1 });
    registry.with_mut(counter, |value| value.value = 9);

    assert_eq!(registry.with(counter, |value| value.value), Some(9));
    assert_eq!(registry.render(counter).unwrap().plain_text_for_test(), "9");
}

#[test]
fn render_attaches_stable_identity_and_clone_preserves_it() {
    let mut registry = ComponentRegistry::new();
    let counter = registry.register(Counter { value: 3 });
    let first = registry.render(counter).unwrap();
    let second = registry.render(counter).unwrap();
    assert_eq!(first.component, Some(counter.id()));
    assert_eq!(first, second);
    assert_eq!(first.clone(), first);
}

#[test]
fn identical_visuals_from_different_components_are_semantically_distinct() {
    let mut registry = ComponentRegistry::new();
    let first = registry.register(SameVisual);
    let second = registry.register(SameVisual);

    assert_ne!(registry.render(first), registry.render(second));
    assert_eq!(
        compile_view(&registry.render(first).unwrap(), 20),
        compile_view(&registry.render(second).unwrap(), 20)
    );
}

#[test]
fn stale_handles_return_none_and_existing_views_remain_compilable() {
    let mut registry = ComponentRegistry::new();
    let counter = registry.register(Counter { value: 4 });
    let view = registry.render(counter).unwrap();
    assert!(registry.remove(counter).is_some());
    assert!(!registry.contains(counter));
    assert_eq!(registry.render(counter), None);
    assert!(!compile_view(&view, 20).rows.is_empty());
}

#[test]
fn structural_transforms_transfer_the_component_root() {
    let id = ComponentId::from_nonzero(std::num::NonZeroU64::new(7).unwrap());
    let source = View::text("x").into_view().with_component(id);

    let container = source.clone().container();
    assert_eq!(container.component, Some(id));
    assert_eq!(child(&container).component, None);

    let clamped = source.clone().clamp_rows(
        2,
        OverflowIndicator::Ellipsis {
            style: StyleSpec::new(),
        },
    );
    assert_eq!(clamped.component, Some(id));
    assert_eq!(clamp_child(&clamped).component, None);

    let boxed = View::box_(source, Decoration::default());
    assert_eq!(boxed.component, Some(id));
    assert_eq!(child(&boxed).component, None);
}

#[test]
fn properties_preserve_the_component_root() {
    let id = ComponentId::from_nonzero(std::num::NonZeroU64::new(8).unwrap());
    let view = View::text("x")
        .into_view()
        .with_component(id)
        .padding(1)
        .bold()
        .fill_width();
    assert_eq!(view.component, Some(id));
}

#[test]
fn nested_component_attachment_wraps_without_overwriting_the_child() {
    let mut registry = ComponentRegistry::new();
    let child_handle = registry.register(SameVisual);
    let child_view = registry.render(child_handle).unwrap();
    let parent = registry.register(Nested { child: child_view });

    let view = registry.render(parent).unwrap();
    assert_eq!(view.component, Some(parent.id()));
    let child_view = child(&view);
    assert_eq!(child_view.component, Some(child_handle.id()));
}

#[test]
fn component_metadata_is_physically_invisible() {
    let mut registry = ComponentRegistry::new();
    let owned = registry.register(SameVisual);
    let with_identity = registry.render(owned).unwrap();
    let without_identity = View::text("same").into_view();

    assert_eq!(
        compile_view(&with_identity, 20),
        compile_view(&without_identity, 20)
    );
}

fn child(view: &View) -> &View {
    let crate::presentation::ir::ViewKind::Container(container) = &view.kind else {
        panic!("expected container")
    };
    &container.child
}

fn clamp_child(view: &View) -> &View {
    let crate::presentation::ir::ViewKind::ClampRows(clamp) = &view.kind else {
        panic!("expected clamp")
    };
    &clamp.child
}

trait TestViewText {
    fn plain_text_for_test(&self) -> String;
}

impl TestViewText for View {
    fn plain_text_for_test(&self) -> String {
        let crate::presentation::ir::ViewKind::Text(text) = &self.kind else {
            panic!("expected text")
        };
        text.spans.iter().map(|span| span.text.as_str()).collect()
    }
}
