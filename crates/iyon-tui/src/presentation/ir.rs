//! Private retained semantic View representation.
//!
//! Construction APIs lower immediately into these owned nodes. This module
//! contains no terminal/backend state.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::{
    component::ComponentId,
    perf::{self, Counter},
};

use super::api::{
    style::{
        BorderSpec, ColorSpec, Insets, OverflowIndicator, StyleFacts, StyleRef, StyleStates,
        VerticalAlign,
    },
    text::{HorizontalAlign, TextSpan, WrapMode},
};

/// Process-local identity for one immutable semantic node.
///
/// Identity is deliberately separate from semantic equality: it is a cache
/// key and a retention cutoff, never part of the public value semantics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ViewId(u64);

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

fn next_view_id() -> ViewId {
    let current = NEXT_VIEW_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .expect("semantic ViewId exhausted");
    ViewId(current)
}

/// Cached facts about a semantic view's recursive payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ViewFlags(u8);

impl ViewFlags {
    const CONTAINS_COMPONENT_SLOT: u8 = 1 << 0;

    pub(crate) const fn contains_component_slot(self) -> bool {
        self.0 & Self::CONTAINS_COMPONENT_SLOT != 0
    }

    const fn with_component_slot() -> Self {
        Self(Self::CONTAINS_COMPONENT_SLOT)
    }
}

/// An owned backend-neutral semantic view.
///
/// Views are persistent values. Cloning one only clones this outer `Arc`; a
/// semantic builder operation allocates a new root identity while retaining
/// every unchanged recursive payload.
#[derive(Debug, PartialEq)]
pub struct View {
    inner: Arc<ViewNode>,
}

#[derive(Debug)]
pub(crate) struct ViewNode {
    id: ViewId,
    flags: ViewFlags,
    pub(crate) component: Option<ComponentId>,
    pub(crate) width: WidthRule,
    pub(crate) height: HeightRule,
    pub(crate) decoration: Decoration,
    pub(crate) style_states: StyleStates,
    pub(crate) style_facts: StyleFacts,
    pub(crate) component_scope: Option<ComponentId>,
    pub(crate) kind: ViewKind,
}

impl Clone for View {
    fn clone(&self) -> Self {
        perf::inc(Counter::ViewCloneCalls);
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub(crate) struct ViewNodeParts {
    pub(crate) component: Option<ComponentId>,
    pub(crate) width: WidthRule,
    pub(crate) height: HeightRule,
    pub(crate) decoration: Decoration,
    pub(crate) style_states: StyleStates,
    pub(crate) style_facts: StyleFacts,
    pub(crate) component_scope: Option<ComponentId>,
    pub(crate) kind: ViewKind,
}

impl View {
    pub(crate) fn from_node(parts: ViewNodeParts) -> Self {
        perf::inc(Counter::ViewNodesConstructedRust);
        Self {
            inner: Arc::new(ViewNode {
                id: next_view_id(),
                flags: ViewNode::compute_flags(parts.component, &parts.kind),
                component: parts.component,
                width: parts.width,
                height: parts.height,
                decoration: parts.decoration,
                style_states: parts.style_states,
                style_facts: parts.style_facts,
                component_scope: parts.component_scope,
                kind: parts.kind,
            }),
        }
    }

    pub(crate) fn id(&self) -> ViewId {
        self.inner.id
    }

    pub(crate) fn view_component(&self) -> Option<ComponentId> {
        self.inner.component
    }

    pub(crate) fn width(&self) -> WidthRule {
        self.inner.width
    }

    pub(crate) fn height(&self) -> HeightRule {
        self.inner.height
    }

    pub(crate) fn decoration(&self) -> &Decoration {
        &self.inner.decoration
    }

    pub(crate) fn view_style_states(&self) -> &StyleStates {
        &self.inner.style_states
    }

    pub(crate) fn view_style_facts(&self) -> &StyleFacts {
        &self.inner.style_facts
    }

    pub(crate) fn component_scope(&self) -> Option<ComponentId> {
        self.inner.component_scope
    }

    pub(crate) fn kind(&self) -> &ViewKind {
        &self.inner.kind
    }

    pub(crate) fn flags(&self) -> ViewFlags {
        self.inner.flags
    }

    pub(crate) fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.inner, &right.inner)
    }

    /// Creates a new semantic root while retaining all unchanged payloads.
    pub(crate) fn map_node(self, update: impl FnOnce(&mut ViewNode)) -> Self {
        let mut next = self.inner.shallow_clone();
        update(&mut next);
        next.id = next_view_id();
        Self {
            inner: Arc::new(next),
        }
    }

    /// Recomputes recursive component facts for a structural/internal update.
    pub(crate) fn map_node_with_flags(self, update: impl FnOnce(&mut ViewNode)) -> Self {
        let mut next = self.inner.shallow_clone();
        update(&mut next);
        next.id = next_view_id();
        next.flags = ViewNode::compute_flags(next.component, &next.kind);
        Self {
            inner: Arc::new(next),
        }
    }

    /// Applies a text-local semantic update without copying its span storage.
    pub(crate) fn map_text(self, update: impl FnOnce(&mut TextView)) -> Self {
        self.map_node(|node| {
            let ViewKind::Text(text) = &mut node.kind else {
                unreachable!("text wrapper must always contain ViewKind::Text")
            };
            update(Arc::make_mut(text));
        })
    }

    pub(crate) fn clone_shell_with(
        &self,
        kind: ViewKind,
        component_scope: Option<ComponentId>,
    ) -> Self {
        Self::from_node(ViewNodeParts {
            component: self.inner.component,
            width: self.inner.width,
            height: self.inner.height,
            decoration: self.inner.decoration.clone(),
            style_states: self.inner.style_states.clone(),
            style_facts: self.inner.style_facts.clone(),
            component_scope,
            kind,
        })
    }

    pub(crate) fn contains_component_identity(&self) -> bool {
        self.flags().contains_component_slot()
    }

    #[cfg(feature = "native-host")]
    pub fn downgrade(&self) -> WeakView {
        WeakView(Arc::downgrade(&self.inner))
    }
}

impl PartialEq for ViewNode {
    fn eq(&self, other: &Self) -> bool {
        self.semantic_eq(other)
    }
}

impl ViewNode {
    fn compute_flags(component: Option<ComponentId>, kind: &ViewKind) -> ViewFlags {
        if component.is_some() {
            return ViewFlags::with_component_slot();
        }
        match kind {
            ViewKind::ComponentSlot(_) => ViewFlags::with_component_slot(),
            ViewKind::Text(_) | ViewKind::Spacer { .. } => ViewFlags::default(),
            ViewKind::Container(container) => container.child.flags(),
            ViewKind::Hanging(hanging) => ViewFlags(
                hanging.prefix.flags().0
                    | hanging.continuation_prefix.flags().0
                    | hanging.body.flags().0,
            ),
            ViewKind::ClampRows(clamp) => clamp.child.flags(),
            ViewKind::RowViewport(viewport) => viewport.child.flags(),
            ViewKind::Column(column) => ViewFlags(
                column
                    .children
                    .iter()
                    .fold(0, |flags, child| flags | child.view.flags().0),
            ),
            ViewKind::Row(row) => ViewFlags(
                row.children
                    .iter()
                    .fold(0, |flags, child| flags | child.view.flags().0),
            ),
            ViewKind::Grid(grid) => ViewFlags(
                grid.cells
                    .iter()
                    .fold(0, |flags, cell| flags | cell.view.flags().0),
            ),
        }
    }

    fn shallow_clone(&self) -> Self {
        Self {
            id: self.id,
            flags: self.flags,
            component: self.component,
            width: self.width,
            height: self.height,
            decoration: self.decoration.clone(),
            style_states: self.style_states.clone(),
            style_facts: self.style_facts.clone(),
            component_scope: self.component_scope,
            kind: self.kind.clone(),
        }
    }

    fn semantic_eq(&self, other: &Self) -> bool {
        self.component == other.component
            && self.width == other.width
            && self.height == other.height
            && self.decoration == other.decoration
            && self.style_states == other.style_states
            && self.style_facts == other.style_facts
            && self.component_scope == other.component_scope
            && self.kind == other.kind
    }
}

#[cfg(feature = "native-host")]
#[derive(Clone)]
pub struct WeakView(std::sync::Weak<ViewNode>);

#[cfg(feature = "native-host")]
impl WeakView {
    pub fn upgrade(&self) -> Option<View> {
        self.0.upgrade().map(|inner| View { inner })
    }
}

/// RETAINED SEMANTIC IR. Generic view node kinds understood by the compiler.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewKind {
    Text(Arc<TextView>),
    Column(Arc<ColumnView>),
    Row(Arc<RowView>),
    Grid(Arc<GridView>),
    Hanging(Arc<HangingView>),
    Container(Arc<ContainerNode>),
    Spacer { rows: u16 },
    ClampRows(Arc<ClampRowsView>),
    RowViewport(Arc<RowViewportView>),
    ComponentSlot(ComponentSlotNode),
}

/// RETAINED SEMANTIC IR. Deferred placement of a retained component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ComponentSlotNode {
    pub(crate) id: ComponentId,
}

/// RETAINED SEMANTIC IR. Width allocation requested from a parent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WidthRule {
    #[default]
    Fit,
    Fill,
}

/// RETAINED SEMANTIC IR. Height allocation requested from a parent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HeightRule {
    #[default]
    Fit,
    Fill,
}
/// RETAINED SEMANTIC IR. Styled text, represented without terminal types.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextView {
    pub(crate) spans: Arc<[TextSpan]>,
    pub(crate) wrap: WrapMode,
    pub(crate) align: HorizontalAlign,
    pub(crate) cursor: Option<TextCursorAnchor>,
}

/// Private semantic caret metadata. It describes a UTF-8 source boundary;
/// layout resolves it to a physical cell without exposing geometry upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextCursorAnchor {
    pub(crate) byte_offset: usize,
}

impl TextView {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![TextSpan::plain(text)].into(),
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            cursor: None,
        }
    }
}
/// RETAINED SEMANTIC IR. Vertical composition. The parent owns sibling gaps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColumnView {
    pub(crate) children: Arc<[ColumnChild]>,
    pub(crate) gap: u16,
}

/// RETAINED SEMANTIC IR. One column child and its height track.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColumnChild {
    pub(crate) track: TrackSize,
    pub(crate) view: View,
}

impl ColumnChild {
    pub(crate) fn content(view: View) -> Self {
        Self {
            track: TrackSize::Content { max: None },
            view,
        }
    }

    pub(crate) fn fixed(height: u16, view: View) -> Self {
        Self {
            track: TrackSize::Fixed(height),
            view,
        }
    }

    pub(crate) fn flex(view: View) -> Self {
        Self {
            track: TrackSize::Flex { min: 1 },
            view,
        }
    }
}

/// RETAINED SEMANTIC IR. Horizontal composition. The parent owns sibling gaps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowView {
    pub(crate) children: Arc<[RowChild]>,
    pub(crate) gap: u16,
    pub(crate) vertical_align: VerticalAlign,
}

/// Semantic first-line prefix plus repeated continuation prefix.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HangingView {
    pub(crate) prefix: View,
    pub(crate) continuation_prefix: View,
    pub(crate) body: View,
}

/// RETAINED SEMANTIC IR. One row child and its width track.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowChild {
    pub(crate) track: TrackSize,
    pub(crate) view: View,
}

impl RowChild {
    pub(crate) fn content(view: View) -> Self {
        Self {
            track: TrackSize::Content { max: None },
            view,
        }
    }

    pub(crate) fn fixed(width: u16, view: View) -> Self {
        Self {
            track: TrackSize::Fixed(width),
            view,
        }
    }

    pub(crate) fn flex(view: View) -> Self {
        Self {
            track: TrackSize::Flex { min: 1 },
            view,
        }
    }
}

/// RETAINED SEMANTIC IR. Shared two-dimensional track layout.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GridView {
    pub(crate) columns: Arc<[TrackSize]>,
    pub(crate) rows: Arc<[TrackSize]>,
    pub(crate) column_gap: u16,
    pub(crate) row_gap: u16,
    pub(crate) cells: Arc<[GridCellView]>,
}

/// RETAINED SEMANTIC IR. One grid cell and its explicit track placement.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GridCellView {
    pub(crate) row: usize,
    pub(crate) column: usize,
    pub(crate) row_span: u16,
    pub(crate) column_span: u16,
    pub(crate) horizontal_align: HorizontalAlign,
    pub(crate) vertical_align: VerticalAlign,
    pub(crate) view: View,
}

/// RETAINED SEMANTIC IR. Width allocation for a row child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrackSize {
    Content { max: Option<u16> },
    Fixed(u16),
    Flex { min: u16 },
    FlexMax { min: u16, max: u16 },
}

/// RETAINED SEMANTIC IR. Structural container holding one semantic child.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContainerNode {
    pub(crate) child: View,
}

/// RETAINED SEMANTIC IR. Common semantic decoration applied by the compiler
/// around a View node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AxisBounds {
    pub(crate) min: u16,
    pub(crate) max: u16,
}

impl Default for AxisBounds {
    fn default() -> Self {
        Self {
            min: 0,
            max: u16::MAX,
        }
    }
}

impl AxisBounds {
    pub(crate) fn normalized_max(self) -> u16 {
        self.max.max(self.min)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ViewBounds {
    pub(crate) width: AxisBounds,
    pub(crate) height: AxisBounds,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Decoration {
    pub(crate) padding: Insets,
    pub(crate) bounds: ViewBounds,
    /// Paints the allocated physical surface, including transparent geometry.
    pub(crate) surface_background: Option<ColorSpec>,
    pub(crate) border: Option<BorderSpec>,
    /// Sparse text intent inherited by descendants and text spans.
    pub(crate) text_style: StyleRef,
}

/// RETAINED SEMANTIC IR. Truncation behavior after physical layout.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClampRowsView {
    pub(crate) child: View,
    pub(crate) max_rows: u16,
    pub(crate) overflow: OverflowIndicator,
}

/// Private physical row crop used by semantic local scroll panes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowViewportView {
    pub(crate) child: View,
    pub(crate) skip_rows: u16,
    /// When set, the viewport contributes this intrinsic height instead of
    /// the child's remaining height. `None` lets the parent provide it.
    pub(crate) visible_height: Option<u16>,
    /// Internal bounded allocation for flexible child layout. This differs
    /// from `visible_height`: the child itself receives this height.
    pub(crate) layout_height: Option<u16>,
    /// Lets a local scroll pane advertise its full content height during
    /// width-only measurement while retaining its allocated viewport height
    /// during bounded layout.
    pub(crate) intrinsic_content_height: bool,
}

#[cfg(test)]
mod tests {
    use super::{View, ViewKind};
    use crate::presentation::IntoView;

    #[test]
    fn clone_retains_identity_and_only_clones_the_outer_arc() {
        let original = View::text("x").into_view();
        let cloned = original.clone();

        assert!(View::ptr_eq(&original, &cloned));
        assert_eq!(original.id(), cloned.id());
        assert_eq!(std::sync::Arc::strong_count(&original.inner), 2);
    }

    #[test]
    fn semantic_mutation_gets_a_new_identity_even_when_unique() {
        let original = View::text("x").into_view();
        let original_id = original.id();
        let changed = original.padding(1);
        assert_ne!(original_id, changed.id());
    }

    #[test]
    fn semantic_mutation_gets_a_new_identity_when_shared() {
        let original = View::text("x").into_view();
        let shared = original.clone();
        let shared_id = shared.id();
        let changed = shared.padding(1);

        assert_ne!(original.id(), changed.id());
        assert_eq!(original.id(), shared_id);
        assert!(!View::ptr_eq(&original, &changed));
    }

    #[test]
    fn semantic_equality_ignores_view_identity() {
        let first = View::text("same").padding(1).into_view();
        let second = View::text("same").padding(1).into_view();

        assert_ne!(first.id(), second.id());
        assert_eq!(first, second);
    }

    #[test]
    fn changing_a_parent_retains_an_unchanged_child_identity() {
        let child = View::text("stable").into_view();
        let root = View::vertical(|column| {
            column.child(child.clone());
        });
        let changed = root.clone().padding(1);

        let ViewKind::Column(original_column) = root.kind() else {
            panic!("expected column root");
        };
        let ViewKind::Column(changed_column) = changed.kind() else {
            panic!("expected column root");
        };
        assert_eq!(original_column.children[0].view.id(), child.id());
        assert_eq!(changed_column.children[0].view.id(), child.id());
        assert!(View::ptr_eq(
            &original_column.children[0].view,
            &changed_column.children[0].view
        ));
    }

    #[test]
    fn component_presence_is_cached_in_flags() {
        let ordinary = View::text("ordinary").into_view();
        assert!(!ordinary.contains_component_identity());

        let mounted = View::vertical(|column| {
            column.child(View::native_component(1));
        });
        assert!(mounted.contains_component_identity());
        assert!(mounted.padding(1).contains_component_identity());
    }

    #[cfg(feature = "native-host")]
    #[test]
    fn weak_bridge_handles_do_not_keep_views_alive() {
        let view = View::text("weak").into_view();
        let weak = view.downgrade();
        let upgraded = weak.upgrade().expect("live view must upgrade");
        assert_eq!(upgraded.id(), view.id());
        drop(upgraded);
        drop(view);
        assert!(weak.upgrade().is_none());
    }
}
