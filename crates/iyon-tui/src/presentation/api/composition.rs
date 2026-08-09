//! Closure-scoped semantic composition capabilities.
//!
//! [`Horizontal`] and [`Vertical`] exist only while their corresponding
//! construction closure runs. Their owned children are lowered immediately by
//! [`View::horizontal`] and [`View::vertical`].

use super::{style::VerticalAlign, view::IntoView};
use crate::presentation::ir::{RowChild, View};

/// Closure-scoped capability for constructing horizontal semantic composition.
///
/// The capability is consumed by `View::horizontal`; it is not a retained
/// semantic node and cannot itself be converted into a `View`.
pub(crate) struct Horizontal {
    children: Vec<RowChild>,
    gap: u16,
    vertical_align: VerticalAlign,
}

impl Horizontal {
    pub(super) fn new() -> Self {
        Self {
            children: Vec::new(),
            gap: 0,
            vertical_align: VerticalAlign::Top,
        }
    }

    pub(crate) fn child(&mut self, child: impl IntoView) -> &mut Self {
        self.children.push(RowChild::content(child.into_view()));
        self
    }

    pub(crate) fn children<I, V>(&mut self, children: I) -> &mut Self
    where
        I: IntoIterator<Item = V>,
        V: IntoView,
    {
        for child in children {
            self.child(child);
        }
        self
    }

    pub(crate) fn fixed(&mut self, width: u16, child: impl IntoView) -> &mut Self {
        self.children
            .push(RowChild::fixed(width, child.into_view()));
        self
    }

    pub(crate) fn flex(&mut self, child: impl IntoView) -> &mut Self {
        self.children.push(RowChild::flex(child.into_view()));
        self
    }

    pub(crate) fn gap(&mut self, gap: u16) -> &mut Self {
        self.gap = gap;
        self
    }

    pub(crate) fn vertical_align(&mut self, align: VerticalAlign) -> &mut Self {
        self.vertical_align = align;
        self
    }

    pub(super) fn into_parts(self) -> (Vec<RowChild>, u16, VerticalAlign) {
        (self.children, self.gap, self.vertical_align)
    }
}

/// Closure-scoped capability for constructing vertical semantic composition.
///
/// The capability is consumed by `View::vertical`; it is not a retained
/// semantic node and cannot itself be converted into a `View`.
pub(crate) struct Vertical {
    children: Vec<View>,
    gap: u16,
}

impl Vertical {
    pub(super) fn new() -> Self {
        Self {
            children: Vec::new(),
            gap: 0,
        }
    }

    pub(crate) fn child(&mut self, child: impl IntoView) -> &mut Self {
        self.children.push(child.into_view());
        self
    }

    pub(crate) fn children<I, V>(&mut self, children: I) -> &mut Self
    where
        I: IntoIterator<Item = V>,
        V: IntoView,
    {
        for child in children {
            self.child(child);
        }
        self
    }

    pub(crate) fn gap(&mut self, gap: u16) -> &mut Self {
        self.gap = gap;
        self
    }

    pub(super) fn into_parts(self) -> (Vec<View>, u16) {
        (self.children, self.gap)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::presentation::api::style::StyleSpec;
    use crate::presentation::api::text::TextSpan;
    use crate::presentation::ir::{ColumnView, RowView, TrackSize, ViewKind, WidthRule};
    use crate::presentation::{IntoView, View};

    fn row(view: &View) -> &RowView {
        let ViewKind::Row(row) = &view.kind else {
            panic!("expected row view");
        };
        row
    }

    fn column(view: &View) -> &ColumnView {
        let ViewKind::Column(column) = &view.kind else {
            panic!("expected column view");
        };
        column
    }

    fn text(view: &View) -> &str {
        let ViewKind::Text(text) = &view.kind else {
            panic!("expected text view");
        };
        &text.spans[0].text
    }

    #[test]
    fn new_semantic_constructors_default_to_fit() {
        let views = [
            View::text("x").into_view(),
            View::styled_text([TextSpan::plain("x")]).into_view(),
            View::horizontal(|_| {}),
            View::vertical(|_| {}),
            View::spacer(1),
        ];

        assert!(views.iter().all(|view| view.width == WidthRule::Fit));
    }

    #[test]
    fn horizontal_defaults_lower_to_fit_row() {
        let view = View::horizontal(|_| {});
        let horizontal = row(&view);

        assert_eq!(view.width, WidthRule::Fit);
        assert_eq!(view.decoration, Default::default());
        assert!(horizontal.children.is_empty());
        assert_eq!(horizontal.gap, 0);
        assert_eq!(horizontal.vertical_align, VerticalAlign::Top);
    }

    #[test]
    fn vertical_defaults_lower_to_fit_column() {
        let view = View::vertical(|_| {});
        let vertical = column(&view);

        assert_eq!(view.width, WidthRule::Fit);
        assert_eq!(view.decoration, Default::default());
        assert!(vertical.children.is_empty());
        assert_eq!(vertical.gap, 0);
    }

    #[test]
    fn horizontal_lowers_content_fixed_and_flex_tracks() {
        let view = View::horizontal(|row| {
            row.child("content");
            row.fixed(5, "fixed");
            row.flex("flex");
        });
        let children = &row(&view).children;

        assert_eq!(children[0].track, TrackSize::Content { max: None });
        assert_eq!(children[1].track, TrackSize::Fixed(5));
        assert_eq!(children[2].track, TrackSize::Flex { min: 1 });
        assert_eq!(text(&children[0].view), "content");
        assert_eq!(text(&children[1].view), "fixed");
        assert_eq!(text(&children[2].view), "flex");
    }

    #[test]
    fn parent_tracks_do_not_mutate_child_width() {
        let fit = View::text("fit");
        let fill = View::text("fill").fill_width();
        let view = View::horizontal(|row| {
            row.fixed(3, fit);
            row.flex(fill);
        });
        let children = &row(&view).children;

        assert_eq!(children[0].view.width, WidthRule::Fit);
        assert_eq!(children[1].view.width, WidthRule::Fill);
    }

    #[test]
    fn builder_properties_are_last_write_wins() {
        let horizontal = View::horizontal(|row| {
            row.gap(1);
            row.gap(3);
            row.vertical_align(VerticalAlign::Center);
            row.vertical_align(VerticalAlign::Bottom);
        });
        assert_eq!(row(&horizontal).gap, 3);
        assert_eq!(row(&horizontal).vertical_align, VerticalAlign::Bottom);

        let vertical = View::vertical(|column| {
            column.gap(1);
            column.gap(3);
        });
        assert_eq!(column(&vertical).gap, 3);
    }

    #[test]
    fn old_and_new_horizontal_construction_have_equal_ir() {
        let old = View::row(
            vec![
                RowChild::content(View::text("a").into_view()),
                RowChild::fixed(4, View::text("b").into_view()),
                RowChild::flex(View::text("c").into_view()),
            ],
            1,
        )
        .width(WidthRule::Fit);
        let new = View::horizontal(|row| {
            row.child("a");
            row.fixed(4, "b");
            row.flex("c");
            row.gap(1);
        });

        assert_eq!(new, old);
    }

    #[test]
    fn old_row_keeps_its_migration_fill_default() {
        assert_eq!(View::row(Vec::new(), 0).width, WidthRule::Fill,);
    }

    #[test]
    fn old_and_new_vertical_construction_have_equal_ir() {
        let old = View::column(
            vec![View::text("a").into_view(), View::text("b").into_view()],
            1,
        );
        let new = View::vertical(|column| {
            column.child("a");
            column.child("b");
            column.gap(1);
        });

        assert_eq!(new, old);
    }

    #[derive(Debug)]
    struct CustomStatus(String);

    impl IntoView for CustomStatus {
        fn into_view(self) -> View {
            View::text(self.0)
                .style(StyleSpec::new().bold())
                .into_view()
        }
    }

    #[test]
    fn children_accept_all_owned_into_view_forms() {
        let existing = View::text("view").into_view();
        let view = View::vertical(|column| {
            column.child("str");
            column.child(String::from("string"));
            column.child(View::text("text"));
            column.child(existing);
            column.child(CustomStatus("custom".into()));
        });
        let children = &column(&view).children;

        assert_eq!(children.len(), 5);
        assert_eq!(text(&children[0]), "str");
        assert_eq!(text(&children[1]), "string");
        assert_eq!(text(&children[2]), "text");
        assert_eq!(text(&children[3]), "view");
        assert_eq!(text(&children[4]), "custom");
        assert_eq!(
            children[4].decoration.text_style.attributes.bold,
            Some(true)
        );
    }

    #[test]
    fn iterator_children_preserve_order() {
        let items = vec!["a", "b", "c"];
        let vertical = View::vertical(|column| {
            column.children(items);
        });
        assert_eq!(
            column(&vertical)
                .children
                .iter()
                .map(text)
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );

        let horizontal = View::horizontal(|row| {
            row.children(["a", "b", "c"]);
        });
        assert_eq!(
            row(&horizontal)
                .children
                .iter()
                .map(|child| text(&child.view))
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn ordinary_rust_control_flow_builds_children() {
        let include_middle = true;
        let view = View::vertical(|column| {
            column.child("a");
            if include_middle {
                column.child("b");
            }
            column.child("c");
        });
        assert_eq!(
            column(&view).children.iter().map(text).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );

        let view = View::vertical(|column| {
            column.child("a");
            if !include_middle {
                column.child("b");
            }
            column.child("c");
        });
        assert_eq!(
            column(&view).children.iter().map(text).collect::<Vec<_>>(),
            ["a", "c"]
        );
    }

    #[test]
    fn helper_functions_can_accept_builder_references() {
        fn add_pair(row: &mut Horizontal) {
            row.child("left").child("right");
        }

        let view = View::horizontal(add_pair);
        assert_eq!(
            row(&view)
                .children
                .iter()
                .map(|child| text(&child.view))
                .collect::<Vec<_>>(),
            ["left", "right"]
        );
    }

    #[test]
    fn closures_execute_immediately_and_only_once() {
        let called = Cell::new(0);
        let view = View::vertical(|column| {
            called.set(called.get() + 1);
            column.child("x");
        });
        assert_eq!(called.get(), 1);
        assert_eq!(text(&column(&view).children[0]), "x");
        let compiler = crate::presentation::internal::ViewCompiler::default();
        let _ = compiler.compile(&view, 1);
        let _ = compiler.compile(&view, 1);
        assert_eq!(called.get(), 1);
    }

    #[test]
    fn borrowed_children_are_owned_before_builder_returns() {
        let mut source = String::from("hello");
        let view = View::vertical(|column| {
            column.child(source.as_str());
        });
        source.clear();
        source.push_str("changed");

        assert_eq!(text(&column(&view).children[0]), "hello");
    }

    #[test]
    fn fn_once_closures_can_move_values_into_composition() {
        let owned = String::from("owned");
        let view = View::vertical(move |column| {
            column.child(owned);
        });
        assert_eq!(text(&column(&view).children[0]), "owned");
    }

    #[test]
    fn legacy_and_new_composition_compile_identically() {
        let old_row = View {
            width: WidthRule::Fit,
            decoration: Default::default(),
            kind: ViewKind::Row(RowView {
                children: vec![
                    RowChild::content(View::text("a").into_view()),
                    RowChild::fixed(4, View::text("b").into_view()),
                    RowChild::flex(View::text("c").into_view()),
                ],
                gap: 1,
                vertical_align: VerticalAlign::Bottom,
            }),
        };
        let new_row = View::horizontal(|row| {
            row.child("a");
            row.fixed(4, "b");
            row.flex("c");
            row.gap(1);
            row.vertical_align(VerticalAlign::Bottom);
        });

        let old_column = View::column(
            vec![View::text("a").into_view(), View::text("b").into_view()],
            1,
        );
        let new_column = View::vertical(|column| {
            column.child("a");
            column.child("b");
            column.gap(1);
        });

        let compiler = crate::presentation::internal::ViewCompiler::default();
        for width in [3, 8, 20] {
            let old = compiler.compile(&old_row, width);
            let new = compiler.compile(&new_row, width);
            assert_eq!(new.rows, old.rows);
            assert_eq!(new.physically_complete, old.physically_complete);
        }
        for width in [3, 8, 20] {
            let old = compiler.compile(&old_column, width);
            let new = compiler.compile(&new_column, width);
            assert_eq!(new.rows, old.rows);
            assert_eq!(new.physically_complete, old.physically_complete);
        }
    }
}
