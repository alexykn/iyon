//! Manual layout and text compiler regressions.

use super::*;
use crate::geometry::{LayoutConstraints, Size};
use crate::physical::{PhysicalColor, PhysicalRow, PhysicalStyle};
use crate::presentation::api::style::{
    BorderEdges, BorderGlyphs, BorderSpec, BorderStyle, OverflowIndicator, TextAttribute,
};
use crate::presentation::ir::ViewKind;
use crate::presentation::ir::{Decoration, RowChild};
use crate::presentation::{
    ColorSpec, HorizontalAlign, Insets, IntoView, StyleSpec, TextSpan, ThemeKey, VerticalAlign,
    View, WidthRule, WrapMode,
};
use crate::theme;

fn text(row: &PhysicalRow) -> String {
    row.plain_text()
}

fn layout_view(view: &View, width: u16, inherited: PhysicalStyle) -> Surface {
    let compiler = ViewCompiler::default();
    let tree = compiler.layout_tree(view, LayoutConstraints::width_only(width));
    ViewPainter.paint_tree_with_style(&compiler, &tree, inherited)
}

fn row_view(children: Vec<RowChild>, gap: u16) -> View {
    View {
        component: None,
        width: WidthRule::Fill,
        height: crate::presentation::ir::HeightRule::Fit,
        decoration: Decoration::default(),
        kind: crate::presentation::ir::ViewKind::Row(crate::presentation::ir::RowView {
            children,
            gap,
            vertical_align: VerticalAlign::Top,
        }),
    }
}

fn box_view(child: View, decoration: Decoration) -> View {
    let mut child = child;
    let component = child.component.take();
    let width = child.width;
    let height = child.height;
    View {
        component,
        width,
        height,
        decoration,
        kind: crate::presentation::ir::ViewKind::Container(
            crate::presentation::ir::ContainerNode {
                child: Box::new(child),
            },
        ),
    }
}

fn background_decoration(color: ColorSpec) -> Decoration {
    Decoration {
        surface_background: Some(color),
        ..Decoration::default()
    }
}

fn background_with_padding(color: ColorSpec, padding: Insets) -> Decoration {
    Decoration {
        surface_background: Some(color),
        padding,
        ..Decoration::default()
    }
}

fn style(color: &str) -> StyleSpec {
    StyleSpec {
        foreground: Some(ColorSpec::Theme(ThemeKey::from(color))),
        ..StyleSpec::default()
    }
}

fn tool_view(body: &str) -> View {
    row_view(
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
                    .fill_width()
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
fn hanging_view_repeats_continuation_prefix_while_body_wraps() {
    let view = View::hanging(
        View::text("10. ").no_wrap(),
        View::text("    ").no_wrap(),
        View::text("one two three").fill_width(),
    )
    .fill_width();
    let block = ViewCompiler::default().compile(&view, 12);

    assert_eq!(
        block
            .rows
            .iter()
            .map(PhysicalRow::plain_text)
            .collect::<Vec<_>>(),
        ["10. one two ", "    three"]
    );
    assert!(block.rows.iter().all(|row| row.width() == 12));
    assert!(block.physically_complete);
}

#[test]
fn hanging_view_marks_prefix_too_wide_as_incomplete_without_panicking() {
    let view = View::hanging(
        View::text("10. ").no_wrap(),
        View::text("    ").no_wrap(),
        View::text("body").fill_width(),
    )
    .fill_width();
    let block = ViewCompiler::default().compile(&view, 3);

    assert!(!block.physically_complete);
    assert!(!block.rows.is_empty());
}

#[test]
fn hanging_view_preserves_prefix_and_continuation_styles() {
    let marker = View::text("* ").no_wrap().foreground(ColorSpec::ansi(3));
    let continuation = View::text("  ").no_wrap().foreground(ColorSpec::ansi(3));
    let view = View::hanging(marker, continuation, View::text("one two").fill_width()).fill_width();
    let rows = ViewCompiler::default().compile(&view, 6).rows;

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].style_at(0), rows[1].style_at(0));
    assert_eq!(rows[0].style_at(1), rows[1].style_at(1));
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
    let view = row_view(
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
    let block = crate::presentation::layout::compile_bounded_view(&view, Size::new(5, 3));
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
    let block = crate::presentation::layout::compile_bounded_view(&view, Size::new(10, 3));
    assert_eq!(block.rows.len(), 3);
    assert_eq!(block.rows[0].plain_text(), "x");
    assert!(block.rows[1].plain_text().is_empty());
}

mod flow;
mod style;
mod text;
