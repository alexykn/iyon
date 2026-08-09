//! Manual layout and text compiler regressions.

use super::*;
use crate::geometry::Size;
use crate::physical::{PhysicalColor, PhysicalRow, PhysicalStyle};
use crate::presentation::api::style::{BorderSpec, BorderStyle, OverflowIndicator, TextAttribute};
use crate::presentation::ir::ViewKind;
use crate::presentation::{
    ColorSpec, Decoration, HorizontalAlign, Insets, IntoView, RowChild, StyleSpec, TextSpan,
    ThemeKey, View, WidthRule, WrapMode,
};
use crate::theme;

fn text(row: &PhysicalRow) -> String {
    row.plain_text()
}

fn style(color: &str) -> StyleSpec {
    StyleSpec {
        foreground: Some(ColorSpec::Theme(ThemeKey::from(color))),
        ..StyleSpec::default()
    }
}

fn tool_view(body: &str) -> View {
    View::row(
        vec![
            RowChild::content(
                View::text("●")
                    .no_wrap()
                    .style(style("tool.running"))
                    .into_view(),
            ),
            RowChild::flex(
                View::text(body)
                    .style(style("text.default"))
                    .width(WidthRule::Fill)
                    .into_view(),
            ),
        ],
        1,
    )
}

fn assert_block_shape(block: &LayoutBlock, width: u16, height: usize) {
    assert_eq!(block.width, width);
    assert_eq!(block.rows.len(), height);
}

fn empty_vertical() -> View {
    View::vertical(|_| {})
}

#[test]
fn bounded_vertical_tracks_allocate_multiple_flex_children() {
    let view = View::vertical(|column| {
        column.fixed(2, View::text("header").fill_height());
        column.flex(View::text("body").fill_height());
        column.flex(View::text("tail").fill_height());
        column.fixed(1, View::text("footer").fill_height());
    })
    .fill_width()
    .fill_height();
    let compiler = ViewCompiler::default();
    let tree = compiler.layout_tree(
        &view,
        crate::geometry::LayoutConstraints::bounded(Size::new(20, 10)),
    );
    assert!(tree.validate());
    let root = tree.node(tree.root);
    let children = root.children.clone();
    assert_eq!(children.len(), 4);
    assert_eq!(tree.node(children[0]).rect.y, 0);
    assert_eq!(tree.node(children[0]).rect.height, 2);
    assert_eq!(tree.node(children[1]).rect.y, 2);
    assert_eq!(tree.node(children[1]).rect.height, 4);
    assert_eq!(tree.node(children[2]).rect.y, 6);
    assert_eq!(tree.node(children[2]).rect.height, 3);
    assert_eq!(tree.node(children[3]).rect.y, 9);
    assert_eq!(tree.node(children[3]).rect.height, 1);
}

#[test]
fn unbounded_column_treats_flex_as_intrinsic_after_fixed_tracks() {
    let view = View::vertical(|column| {
        column.fixed(3, View::text("header").fill_height());
        column.child(View::text("content"));
        column.flex(View::text("body\nline\nthree").fill_height());
    });
    let tree = ViewCompiler::default()
        .layout_tree(&view, crate::geometry::LayoutConstraints::width_only(20));
    let root = tree.node(tree.root);
    assert_eq!(root.rect.height, 7);
    assert_eq!(tree.node(root.children[0]).rect.height, 3);
    assert_eq!(tree.node(root.children[1]).rect.height, 1);
    assert_eq!(tree.node(root.children[2]).rect.height, 3);
}

#[test]
fn fit_row_respects_fixed_track_and_fill_width_content() {
    let view = View::row(
        vec![
            RowChild::fixed(5, View::text("fixed").fill_width().into_view()),
            RowChild::content(View::text("x").fill_width().into_view()),
        ],
        0,
    )
    .fit_width();
    let tree = ViewCompiler::default()
        .layout_tree(&view, crate::geometry::LayoutConstraints::width_only(20));
    let root = tree.node(tree.root);
    assert_eq!(root.rect.width, 6);
    assert_eq!(tree.node(root.children[0]).rect.width, 5);
    assert_eq!(tree.node(root.children[1]).rect.width, 1);
}

#[test]
fn bounded_row_vertical_alignment_uses_extra_height() {
    let view = View::horizontal(|row| {
        row.child(View::text("x"));
        row.vertical_align(crate::presentation::VerticalAlign::Bottom);
    })
    .fill_width()
    .fill_height();
    let block = ViewCompiler::default().compile_bounded(&view, Size::new(5, 3));
    assert!(block.rows[0].plain_text().is_empty());
    assert!(block.rows[1].plain_text().is_empty());
    assert_eq!(block.rows[2].plain_text(), "x");
}

#[test]
fn clamp_does_not_mask_impossible_wide_grapheme() {
    let view = View::text("漢")
        .fill_width()
        .clamp_rows(1, OverflowIndicator::None);
    assert!(
        !ViewCompiler::default()
            .compile(&view, 1)
            .physically_complete
    );
}

#[test]
fn bounded_compiler_preserves_fit_height_inside_fixed_track() {
    let view = View::vertical(|column| {
        column.fixed(3, View::text("x"));
    })
    .fill_height();
    let block = ViewCompiler::default().compile_bounded(&view, Size::new(10, 3));
    assert_eq!(block.rows.len(), 3);
    assert_eq!(block.rows[0].plain_text(), "x");
    assert!(block.rows[1].plain_text().is_empty());
}

mod flow;
mod style;
mod text;
