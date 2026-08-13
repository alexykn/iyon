//! Semantic View construction and the open conversion boundary.

use super::{
    composition::{Horizontal, Vertical},
    style::{
        BorderSpec, ColorSpec, Insets, OverflowIndicator, StyleFacts, StyleRef, StyleSpec,
        StyleStateKey, StyleStateValue, StyleStates, TextAttribute,
    },
    text::{Text, TextSpan},
};
use crate::presentation::ir::{
    ClampRowsView, ColumnChild, ColumnView, ContainerNode, Decoration, HangingView, HeightRule,
    RowView, View, ViewKind, WidthRule,
};

impl View {
    pub(crate) fn new_kind(kind: ViewKind) -> Self {
        Self {
            component: None,
            width: WidthRule::Fit,
            height: HeightRule::Fit,
            decoration: Decoration::default(),
            style_states: StyleStates::default(),
            style_facts: StyleFacts::default(),
            component_scope: None,
            kind,
        }
    }

    fn wrap_structural(self, make_kind: impl FnOnce(Box<View>) -> ViewKind) -> Self {
        let mut child = self;
        let component = child.component.take();
        let width = child.width;
        let height = child.height;
        Self {
            component,
            width,
            height,
            decoration: Decoration::default(),
            style_states: StyleStates::default(),
            style_facts: StyleFacts::default(),
            component_scope: None,
            kind: make_kind(Box::new(child)),
        }
    }

    pub fn text(text: impl Into<String>) -> Text {
        Text::plain(text)
    }

    pub fn styled_text(spans: impl IntoIterator<Item = TextSpan>) -> Text {
        Text::styled(spans)
    }

    /// Constructs horizontal composition immediately with a `Fit` width.
    /// The builder defaults to zero gap and top vertical alignment.
    pub fn horizontal(build: impl FnOnce(&mut Horizontal)) -> Self {
        let mut horizontal = Horizontal::new();
        build(&mut horizontal);
        let (children, gap, vertical_align) = horizontal.into_parts();

        Self::new_kind(ViewKind::Row(RowView {
            children,
            gap,
            vertical_align,
        }))
    }

    /// Constructs vertical composition immediately with a `Fit` width and
    /// zero gap.
    pub fn vertical(build: impl FnOnce(&mut Vertical)) -> Self {
        let mut vertical = Vertical::new();
        build(&mut vertical);
        let (children, gap) = vertical.into_parts();

        Self::new_kind(ViewKind::Column(ColumnView { children, gap }))
    }

    /// Constructs a semantic hanging row.
    ///
    /// Constructs `prefix + hanging body + repeated continuation indentation`.
    ///
    /// The `prefix` is rendered once on the first body row. The
    /// `continuation_prefix` is repeated beside subsequent body rows. The
    /// `body` is an ordinary semantic view, resolved and mounted once, and
    /// may contain Components. Hanging lays the body out once; repetition is
    /// presentation of the continuation prefix rather than repeated content.
    ///
    /// The continuation prefix cannot contain component identity because it
    /// is physically repeated under the current one-placement component
    /// model. Prefix and body Components are supported normally.
    ///
    /// If the prefix leaves no body capacity, the compiled view is marked
    /// physically incomplete rather than silently dropping or relocating body
    /// content. Rendering remains panic-free; terminal-size rejection is a
    /// separate policy.
    pub fn hanging(
        prefix: impl IntoView,
        continuation_prefix: impl IntoView,
        body: impl IntoView,
    ) -> Self {
        let prefix = prefix.into_view();
        let continuation_prefix = continuation_prefix.into_view();
        let body = body.into_view();
        assert!(
            !continuation_prefix.contains_component_identity(),
            "hanging continuation_prefix cannot contain component identity: it is repeated"
        );

        Self::new_kind(ViewKind::Hanging(HangingView {
            prefix: Box::new(prefix),
            continuation_prefix: Box::new(continuation_prefix),
            body: Box::new(body),
        }))
    }

    pub(crate) fn column(children: Vec<View>, gap: u16) -> Self {
        Self::new_kind(ViewKind::Column(ColumnView {
            children: children.into_iter().map(ColumnChild::content).collect(),
            gap,
        }))
    }

    /// Creates a new undecorated structural boundary around this view.
    pub fn container(self) -> Self {
        self.wrap_structural(|child| ViewKind::Container(ContainerNode { child }))
    }

    /// Creates a private vertical viewport around a semantic view.
    pub(crate) fn row_viewport(child: View, skip_rows: u16) -> Self {
        Self {
            component: None,
            width: WidthRule::Fill,
            height: HeightRule::Fill,
            decoration: Decoration::default(),
            style_states: StyleStates::default(),
            style_facts: StyleFacts::default(),
            component_scope: None,
            kind: ViewKind::RowViewport(crate::presentation::ir::RowViewportView {
                child: Box::new(child),
                skip_rows,
                visible_height: None,
                layout_height: None,
                intrinsic_content_height: true,
            }),
        }
    }

    /// Creates a private vertical viewport with an explicit visible height.
    /// The child is still laid out at its full intrinsic height so component
    /// allocation remains truthful while painting and visibility are clipped.
    pub(crate) fn row_viewport_with_height(
        child: View,
        skip_rows: u16,
        visible_height: Option<u16>,
    ) -> Self {
        Self {
            component: None,
            width: WidthRule::Fill,
            height: HeightRule::Fill,
            decoration: Decoration::default(),
            style_states: StyleStates::default(),
            style_facts: StyleFacts::default(),
            component_scope: None,
            kind: ViewKind::RowViewport(crate::presentation::ir::RowViewportView {
                child: Box::new(child),
                skip_rows,
                visible_height,
                layout_height: None,
                intrinsic_content_height: false,
            }),
        }
    }

    pub(crate) fn bounded_row_viewport(child: View, height: u16) -> Self {
        Self {
            component: None,
            width: WidthRule::Fill,
            height: HeightRule::Fill,
            decoration: Decoration::default(),
            style_states: StyleStates::default(),
            style_facts: StyleFacts::default(),
            component_scope: None,
            kind: ViewKind::RowViewport(crate::presentation::ir::RowViewportView {
                child: Box::new(child),
                skip_rows: 0,
                visible_height: Some(height),
                layout_height: Some(height),
                intrinsic_content_height: false,
            }),
        }
    }

    /// Creates a new structural truncation boundary around this view.
    pub fn clamp_rows(self, max_rows: u16, overflow: OverflowIndicator) -> Self {
        self.wrap_structural(|child| {
            ViewKind::ClampRows(ClampRowsView {
                child,
                max_rows,
                overflow,
            })
        })
    }

    pub fn spacer(rows: u16) -> Self {
        Self::new_kind(ViewKind::Spacer { rows })
    }

    /// Assigns one application-owned semantic styling dimension to this View
    /// subtree. Appearance is resolved later during painting.
    pub fn style_state(
        mut self,
        key: impl Into<StyleStateKey>,
        value: impl Into<StyleStateValue>,
    ) -> Self {
        self.style_states.set(key, value);
        self
    }

    /// Assigns a self-only semantic styling fact. This is crate-private until
    /// the typed text selector API is introduced.
    #[allow(dead_code)]
    pub(crate) fn style_fact(
        mut self,
        key: impl Into<StyleStateKey>,
        value: impl Into<StyleStateValue>,
    ) -> Self {
        self.style_facts.set(key, value);
        self
    }

    /// Assigns multiple application-owned semantic styling dimensions.
    pub fn style_states(
        mut self,
        states: impl IntoIterator<Item = (StyleStateKey, StyleStateValue)>,
    ) -> Self {
        for (key, value) in states {
            self = self.style_state(key, value);
        }
        self
    }

    /// Sets the current node's padding; repeated calls replace the prior value.
    pub fn padding(mut self, padding: impl Into<Insets>) -> Self {
        self.decoration.padding = padding.into();
        self
    }

    /// Paints the current node's allocated surface, including transparent tails.
    pub fn background(mut self, color: ColorSpec) -> Self {
        self.decoration.surface_background = Some(color);
        self
    }

    /// Applies a semantic named style or direct sparse style to this node.
    pub fn style(mut self, style: impl Into<StyleRef>) -> Self {
        let style = style.into();
        if style.theme.is_some() {
            self.decoration.text_style = style;
        } else {
            self.decoration.text_style.overlay(&style.local);
        }
        self
    }

    /// Sets inherited foreground intent for descendant text.
    pub fn foreground(mut self, color: ColorSpec) -> Self {
        self.decoration
            .text_style
            .overlay(&StyleSpec::new().foreground(color));
        self
    }

    /// Replaces the current node's complete border specification.
    pub fn border(mut self, border: BorderSpec) -> Self {
        self.decoration.border = Some(border);
        self
    }

    /// Sets sparse inherited text-attribute intent, including explicit false.
    pub fn text_attribute(mut self, attribute: TextAttribute, enabled: bool) -> Self {
        self.decoration
            .text_style
            .overlay(&StyleSpec::new().attribute(attribute, enabled));
        self
    }

    pub fn bold(self) -> Self {
        self.text_attribute(TextAttribute::Bold, true)
    }

    pub fn dim(self) -> Self {
        self.text_attribute(TextAttribute::Dim, true)
    }

    pub fn italic(self) -> Self {
        self.text_attribute(TextAttribute::Italic, true)
    }

    pub fn underline(self) -> Self {
        self.text_attribute(TextAttribute::Underline, true)
    }

    pub fn reversed(self) -> Self {
        self.text_attribute(TextAttribute::Reversed, true)
    }

    pub fn fit_width(mut self) -> Self {
        self.width = WidthRule::Fit;
        self
    }

    pub fn fill_width(mut self) -> Self {
        self.width = WidthRule::Fill;
        self
    }

    pub fn fit_height(mut self) -> Self {
        self.height = HeightRule::Fit;
        self
    }

    pub fn fill_height(mut self) -> Self {
        self.height = HeightRule::Fill;
        self
    }

    /// Sets the minimum outer width this View may receive.
    pub fn min_width(mut self, width: u16) -> Self {
        self.decoration.bounds.width.min = width;
        self
    }

    /// Sets the maximum outer width this View may receive.
    pub fn max_width(mut self, width: u16) -> Self {
        self.decoration.bounds.width.max = width;
        self
    }

    /// Sets the minimum outer height this View may receive.
    pub fn min_height(mut self, height: u16) -> Self {
        self.decoration.bounds.height.min = height;
        self
    }

    /// Sets the maximum outer height this View may receive.
    pub fn max_height(mut self, height: u16) -> Self {
        self.decoration.bounds.height.max = height;
        self
    }
}

/// Explicit conversion from semantic construction values into the canonical
/// owned [`View`] representation.
pub trait IntoView {
    fn into_view(self) -> View;
}

impl IntoView for View {
    fn into_view(self) -> View {
        self
    }
}

impl IntoView for Text {
    fn into_view(self) -> View {
        self.into_canonical_view()
    }
}

impl IntoView for String {
    fn into_view(self) -> View {
        View::text(self).into_view()
    }
}

impl<'a> IntoView for &'a str {
    fn into_view(self) -> View {
        View::text(self).into_view()
    }
}

impl<'a> IntoView for &'a String {
    fn into_view(self) -> View {
        View::text(self.as_str()).into_view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::api::style::{
        BorderSpec, ColorSpec, Insets, OverflowIndicator, StyleSpec, TextAttribute,
    };
    use crate::presentation::api::text::{HorizontalAlign, Text, WrapMode};

    #[test]
    fn into_view_conversions_are_owned_and_view_conversion_is_identity() {
        let original = View::column(vec![View::spacer(1)], 0);
        assert_eq!(original.clone().into_view(), original);

        let string_view = String::from("hello").into_view();
        let borrowed_source = String::from("hello");
        let borrowed_view = (&borrowed_source).into_view();
        let str_view = "hello".into_view();
        let expected = View::text("hello").into_view();
        assert_eq!(string_view, expected);
        assert_eq!(borrowed_view, expected);
        assert_eq!(str_view, expected);

        let mut source = String::from("owned");
        let view = (&source).into_view();
        source.clear();
        source.push_str("changed");
        let ViewKind::Text(text) = view.kind else {
            panic!("expected text view");
        };
        assert_eq!(text.spans[0].text, "owned");
    }

    #[derive(Debug)]
    struct CustomStatus {
        value: String,
    }

    impl IntoView for CustomStatus {
        fn into_view(self) -> View {
            View::text(self.value)
                .style(StyleSpec::new().bold())
                .into_view()
        }
    }

    #[test]
    fn custom_into_view_implementation_uses_open_boundary() {
        let view = CustomStatus {
            value: "status".to_string(),
        }
        .into_view();
        assert_eq!(view.decoration.text_style.attributes.bold, Some(true));
        assert!(matches!(view.kind, ViewKind::Text(_)));
    }

    #[test]
    fn semantic_composition_is_owned_data() {
        let view = View::horizontal(|row| {
            row.child(View::text("●").no_wrap());
            row.flex(View::text("long command").fill_width());
            row.gap(1);
        })
        .background(ColorSpec::Theme("accent".into()))
        .padding(Insets::all(1));
        assert!(matches!(view.kind, ViewKind::Row(_)));
        assert!(!view.decoration.padding.eq(&Insets::ZERO));
    }

    #[test]
    fn view_properties_mutate_the_current_node_and_are_last_write_wins() {
        let view = View::vertical(|_| {})
            .fit_width()
            .padding(1)
            .padding(Insets::vertical(2))
            .background(ColorSpec::ansi(1))
            .background(ColorSpec::ansi(2))
            .foreground(ColorSpec::ansi(3))
            .foreground(ColorSpec::ansi(4))
            .border(BorderSpec::plain())
            .border(BorderSpec::rounded().color(ColorSpec::ansi(5)))
            .bold()
            .dim()
            .italic()
            .underline()
            .reversed()
            .text_attribute(TextAttribute::Bold, false);

        assert!(matches!(view.kind, ViewKind::Column(_)));
        assert_eq!(view.width, WidthRule::Fit);
        assert_eq!(view.decoration.padding, Insets::vertical(2));
        assert_eq!(view.decoration.surface_background, Some(ColorSpec::ansi(2)));
        assert_eq!(
            view.decoration.text_style.foreground,
            Some(ColorSpec::ansi(4))
        );
        assert_eq!(view.decoration.text_style.attributes.bold, Some(false));
        assert_eq!(view.decoration.text_style.attributes.dim, Some(true));
        assert_eq!(view.decoration.text_style.attributes.italic, Some(true));
        assert_eq!(view.decoration.text_style.attributes.underline, Some(true));
        assert_eq!(view.decoration.text_style.attributes.reversed, Some(true));
        assert_eq!(
            view.decoration.border,
            Some(BorderSpec::rounded().color(ColorSpec::ansi(5)))
        );
    }

    #[test]
    fn independent_properties_are_structurally_commutative() {
        let first = View::vertical(|_| {})
            .padding(Insets::horizontal(1))
            .background(ColorSpec::ansi(1))
            .foreground(ColorSpec::ansi(2))
            .bold();
        let second = View::vertical(|_| {})
            .bold()
            .foreground(ColorSpec::ansi(2))
            .background(ColorSpec::ansi(1))
            .padding(Insets::horizontal(1));

        assert_eq!(first, second);
    }

    #[test]
    fn container_creates_an_outer_boundary_without_moving_child_properties() {
        let property_before = View::text("x").padding(1).container();
        let property_after = View::text("x").container().padding(1);

        let ViewKind::Container(first) = &property_before.kind else {
            panic!("expected container");
        };
        let ViewKind::Container(second) = &property_after.kind else {
            panic!("expected container");
        };
        assert_eq!(first.child.decoration.padding, Insets::all(1));
        assert_eq!(property_before.decoration, Decoration::default());
        assert_eq!(second.child.decoration, Decoration::default());
        assert_eq!(property_after.decoration.padding, Insets::all(1));
    }

    #[test]
    fn structural_transforms_copy_width_and_remain_nested() {
        let inner_fill = View::text("x").fill_width().container();
        let outer_fill = View::text("x").container().fill_width();

        let ViewKind::Container(inner_fill_node) = &inner_fill.kind else {
            panic!("expected container");
        };
        let ViewKind::Container(outer_fill_node) = &outer_fill.kind else {
            panic!("expected container");
        };
        assert_eq!(inner_fill.width, WidthRule::Fill);
        assert_eq!(inner_fill_node.child.width, WidthRule::Fill);
        assert_eq!(outer_fill.width, WidthRule::Fill);
        assert_eq!(outer_fill_node.child.width, WidthRule::Fit);

        let inner_fill = View::text("x").fill_height().container();
        let outer_fill = View::text("x").container().fill_height();
        let ViewKind::Container(inner_fill_node) = &inner_fill.kind else {
            panic!("expected container");
        };
        let ViewKind::Container(outer_fill_node) = &outer_fill.kind else {
            panic!("expected container");
        };
        assert_eq!(inner_fill.height, crate::presentation::ir::HeightRule::Fill);
        assert_eq!(
            inner_fill_node.child.height,
            crate::presentation::ir::HeightRule::Fill
        );
        assert_eq!(outer_fill.height, crate::presentation::ir::HeightRule::Fill);
        assert_eq!(
            outer_fill_node.child.height,
            crate::presentation::ir::HeightRule::Fit
        );

        let nested = View::text("x").container().container();
        let ViewKind::Container(outer) = nested.kind else {
            panic!("expected outer container");
        };
        assert!(matches!(outer.child.kind, ViewKind::Container(_)));
    }

    #[test]
    fn clamp_is_a_structural_transform_with_a_copied_width() {
        let view = View::text("x")
            .fill_width()
            .clamp_rows(2, OverflowIndicator::None);
        let ViewKind::ClampRows(clamp) = &view.kind else {
            panic!("expected clamp");
        };

        assert_eq!(view.width, WidthRule::Fill);
        assert_eq!(clamp.child.width, WidthRule::Fill);
        assert_eq!(view.decoration, Decoration::default());
        assert!(matches!(clamp.child.kind, ViewKind::Text(_)));
    }

    #[test]
    fn text_properties_keep_the_typed_boundary_until_structural_transform() {
        fn accepts_text(_: Text) {}

        let text = View::text("x")
            .padding(1)
            .background(ColorSpec::ansi(1))
            .foreground(ColorSpec::ansi(2))
            .border(BorderSpec::plain())
            .bold()
            .dim()
            .italic()
            .underline()
            .reversed()
            .text_attribute(TextAttribute::Bold, false)
            .no_wrap()
            .text_align(HorizontalAlign::End)
            .fill_width();
        accepts_text(text.clone());

        let view = text.clone().into_view();
        let ViewKind::Text(text_view) = &view.kind else {
            panic!("expected text");
        };
        assert_eq!(text_view.wrap, WrapMode::NoWrap);
        assert_eq!(view.width, WidthRule::Fill);
        let view = text.container();
        assert!(matches!(view.kind, ViewKind::Container(_)));
    }
}
