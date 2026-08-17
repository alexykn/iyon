//! Private retained semantic View representation.
//!
//! Construction APIs lower immediately into these owned nodes. This module
//! contains no terminal/backend state.

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

/// An owned backend-neutral semantic view.
///
/// Views can be composed, styled, cloned, stored, and returned without
/// exposing terminal state or layout implementation details.
#[derive(Debug, PartialEq)]
pub struct View {
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
        perf::inc(Counter::ViewNodesDeepCopied);
        Self {
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
}

impl View {
    pub(crate) fn clone_shell_with(
        &self,
        kind: ViewKind,
        component_scope: Option<ComponentId>,
    ) -> Self {
        perf::inc(Counter::ViewNodesConstructedRust);
        Self {
            component: self.component,
            width: self.width,
            height: self.height,
            decoration: self.decoration.clone(),
            style_states: self.style_states.clone(),
            style_facts: self.style_facts.clone(),
            component_scope,
            kind,
        }
    }

    pub(crate) fn contains_component_identity(&self) -> bool {
        if self.component.is_some() {
            return true;
        }
        match &self.kind {
            ViewKind::ComponentSlot(_) => true,
            ViewKind::Text(_) | ViewKind::Spacer { .. } => false,
            ViewKind::Container(container) => container.child.contains_component_identity(),
            ViewKind::Hanging(hanging) => {
                hanging.prefix.contains_component_identity()
                    || hanging.continuation_prefix.contains_component_identity()
                    || hanging.body.contains_component_identity()
            }
            ViewKind::ClampRows(clamp) => clamp.child.contains_component_identity(),
            ViewKind::RowViewport(viewport) => viewport.child.contains_component_identity(),
            ViewKind::Column(column) => column
                .children
                .iter()
                .any(|child| child.view.contains_component_identity()),
            ViewKind::Row(row) => row
                .children
                .iter()
                .any(|child| child.view.contains_component_identity()),
            ViewKind::Grid(grid) => grid
                .cells
                .iter()
                .any(|cell| cell.view.contains_component_identity()),
        }
    }
}
/// RETAINED SEMANTIC IR. Generic view node kinds understood by the compiler.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewKind {
    Text(TextView),
    Column(ColumnView),
    Row(RowView),
    Grid(GridView),
    Hanging(HangingView),
    Container(ContainerNode),
    Spacer { rows: u16 },
    ClampRows(ClampRowsView),
    RowViewport(RowViewportView),
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
    pub(crate) spans: Vec<TextSpan>,
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
            spans: vec![TextSpan::plain(text)],
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            cursor: None,
        }
    }
}
/// RETAINED SEMANTIC IR. Vertical composition. The parent owns sibling gaps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColumnView {
    pub(crate) children: Vec<ColumnChild>,
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
    pub(crate) children: Vec<RowChild>,
    pub(crate) gap: u16,
    pub(crate) vertical_align: VerticalAlign,
}

/// Semantic first-line prefix plus repeated continuation prefix.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HangingView {
    pub(crate) prefix: Box<View>,
    pub(crate) continuation_prefix: Box<View>,
    pub(crate) body: Box<View>,
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
    pub(crate) columns: Vec<TrackSize>,
    pub(crate) rows: Vec<TrackSize>,
    pub(crate) column_gap: u16,
    pub(crate) row_gap: u16,
    pub(crate) cells: Vec<GridCellView>,
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
    pub(crate) child: Box<View>,
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
    pub(crate) child: Box<View>,
    pub(crate) max_rows: u16,
    pub(crate) overflow: OverflowIndicator,
}

/// Private physical row crop used by semantic local scroll panes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowViewportView {
    pub(crate) child: Box<View>,
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
