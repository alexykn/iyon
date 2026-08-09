//! Semantic View construction and the open conversion boundary.

use super::{
    composition::{Horizontal, Vertical},
    style::{OverflowIndicator, VerticalAlign},
    text::{Text, TextSpan},
};
use crate::presentation::ir::{
    ClampRowsView, ColumnView, ContainerNode, Decoration, RowChild, RowView, View, ViewKind,
    WidthRule,
};

impl View {
    pub(crate) fn text(text: impl Into<String>) -> Text {
        Text::plain(text)
    }

    pub(crate) fn styled_text(spans: impl IntoIterator<Item = TextSpan>) -> Text {
        Text::styled(spans)
    }

    /// Constructs horizontal composition immediately with a `Fit` width.
    /// The builder defaults to zero gap and top vertical alignment.
    pub(crate) fn horizontal(build: impl FnOnce(&mut Horizontal)) -> Self {
        let mut horizontal = Horizontal::new();
        build(&mut horizontal);
        let (children, gap, vertical_align) = horizontal.into_parts();

        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Row(RowView {
                children,
                gap,
                vertical_align,
            }),
        }
    }

    /// Constructs vertical composition immediately with a `Fit` width and
    /// zero gap.
    pub(crate) fn vertical(build: impl FnOnce(&mut Vertical)) -> Self {
        let mut vertical = Vertical::new();
        build(&mut vertical);
        let (children, gap) = vertical.into_parts();

        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Column(ColumnView { children, gap }),
        }
    }

    pub(crate) fn column(children: Vec<View>, gap: u16) -> Self {
        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Column(ColumnView { children, gap }),
        }
    }

    pub(crate) fn row(children: Vec<RowChild>, gap: u16) -> Self {
        Self {
            width: WidthRule::Fill,
            decoration: Decoration::default(),
            kind: ViewKind::Row(RowView {
                children,
                gap,
                vertical_align: VerticalAlign::Top,
            }),
        }
    }

    pub(crate) fn box_(child: View, decoration: Decoration) -> Self {
        let width = child.width;
        Self {
            width,
            decoration,
            kind: ViewKind::Container(ContainerNode {
                child: Box::new(child),
            }),
        }
    }

    pub(crate) fn spacer(rows: u16) -> Self {
        Self {
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::Spacer { rows },
        }
    }

    pub(crate) fn clamp_rows(child: View, max_rows: u16, overflow: OverflowIndicator) -> Self {
        Self {
            width: child.width,
            decoration: Decoration::default(),
            kind: ViewKind::ClampRows(ClampRowsView {
                child: Box::new(child),
                max_rows,
                overflow,
            }),
        }
    }

    pub(crate) fn fit_width(mut self) -> Self {
        self.width = WidthRule::Fit;
        self
    }

    pub(crate) fn fill_width(mut self) -> Self {
        self.width = WidthRule::Fill;
        self
    }

    /// Migration-only internal sizing method. Replaced by `fit_width` and
    /// `fill_width` in the final semantic API.
    pub(crate) fn width(mut self, width: WidthRule) -> Self {
        self.width = width;
        self
    }
}

/// Explicit conversion from semantic construction values into the canonical
/// owned [`View`] representation.
pub(crate) trait IntoView {
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
    use crate::presentation::api::style::{ColorSpec, Insets, StyleSpec};

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
    fn row_and_container_are_owned_data() {
        let view = View::box_(
            View::row(
                vec![
                    RowChild::content(View::text("●").no_wrap().into_view()),
                    RowChild::flex(
                        View::text("long command")
                            .width(WidthRule::Fill)
                            .into_view(),
                    ),
                ],
                1,
            ),
            Decoration::background(ColorSpec::Theme("tool.running".into())).padding(Insets::all(1)),
        );
        assert!(matches!(view.kind, ViewKind::Container(_)));
        assert!(!view.decoration.padding.eq(&Insets::ZERO));
    }
}
