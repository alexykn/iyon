//! Manual layout and text compiler regressions.

use super::*;
use crate::physical::{PhysicalColor, PhysicalRow, PhysicalStyle};
use crate::presentation::api::style::{BorderSpec, BorderStyle, OverflowIndicator, TextAttribute};
use crate::presentation::ir::ViewKind;
use crate::presentation::stream::StreamRowCommit;
use crate::presentation::{
    ColorSpec, Decoration, ExactTerminator, HorizontalAlign, Insets, IntoView, ProjectedText,
    ProjectedTextLayout, ProjectedTextRun, RowChild, StreamNode, StreamOffset, StreamRange,
    StreamView, StyleSpec, TextSpan, ThemeKey, View, WidthRule, WrapMode,
};
use crate::theme;

fn range(start: u64, end: u64) -> StreamRange {
    StreamRange::new(StreamOffset::new(start), StreamOffset::new(end))
}

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

#[test]
fn row_uses_track_width_for_continuations() {
    let rows = compile_view(&tool_view("abcdefghijklmnop"), 10).rows;
    assert_eq!(text(&rows[0]), "● abcdefgh");
    assert_eq!(text(&rows[1]), "  ijklmnop");
}

#[test]
fn narrow_rows_never_overflow_the_surface() {
    for width in 0..=4 {
        let view = View::row(
            vec![
                RowChild::fixed(3, View::text("abc").into_view()),
                RowChild::flex(View::text("body").width(WidthRule::Fill).into_view()),
                RowChild::content(View::text("status").into_view()),
            ],
            2,
        );
        let block = compile_view(&view, width);
        assert!(block.width <= width);
        for row in block.rows {
            assert!(row.width() <= usize::from(width));
        }
    }
}

#[test]
fn styled_spans_survive_wrapping_and_newlines() {
    let view = View::styled_text(vec![
        TextSpan::styled("abc", style("tool.running")),
        TextSpan::styled("def\ngh", style("tool.error")),
    ])
    .width(WidthRule::Fill)
    .into_view();
    let rows = compile_view(&view, 4).rows;
    assert_eq!(text(&rows[0]), "abcd");
    assert_eq!(text(&rows[1]), "ef");
    assert_eq!(text(&rows[2]), "gh");
    assert_eq!(
        rows[0].style_at(0).and_then(|style| style.foreground),
        theme::physical_tool_running().foreground
    );
    assert_eq!(
        rows[0].style_at(3).and_then(|style| style.foreground),
        theme::physical_tool_error().foreground
    );
    assert_eq!(
        rows[2].style_at(0).and_then(|style| style.foreground),
        theme::physical_tool_error().foreground
    );
}

#[test]
fn typed_text_style_cascades_to_physical_spans_without_rewriting_them() {
    let text = View::styled_text([
        TextSpan::plain("plain"),
        TextSpan::styled("bold", StyleSpec::new().bold()),
    ])
    .style(StyleSpec::new().foreground(ColorSpec::Ansi(1)))
    .into_view();
    let rows = compile_view(&text, 20).rows;

    assert_eq!(
        rows[0].style_at(0).and_then(|style| style.foreground),
        Some(PhysicalColor::Indexed(1))
    );
    assert!(!rows[0].style_at(0).is_some_and(|style| style.bold));
    assert_eq!(
        rows[0].style_at(5).and_then(|style| style.foreground),
        Some(PhysicalColor::Indexed(1))
    );
    assert!(rows[0].style_at(5).is_some_and(|style| style.bold));
}

#[test]
fn typed_text_wrap_and_no_wrap_preserve_existing_behavior() {
    let wrapped = View::text("abcd efgh")
        .wrap(WrapMode::WordThenGrapheme)
        .into_view();
    let grapheme = View::text("abcd efgh").wrap(WrapMode::Grapheme).into_view();
    let no_wrap = View::text("abcdef").no_wrap().into_view();

    let ViewKind::Text(wrapped_text) = wrapped.kind else {
        panic!("expected text view");
    };
    let ViewKind::Text(grapheme_text) = grapheme.kind else {
        panic!("expected text view");
    };
    assert_eq!(wrapped_text.wrap, WrapMode::WordThenGrapheme);
    assert_eq!(grapheme_text.wrap, WrapMode::Grapheme);
    assert!(
        !ViewCompiler::default()
            .compile(&no_wrap, 3)
            .physically_complete
    );
}

#[test]
fn typed_text_alignment_uses_existing_text_layout() {
    for (align, expected) in [
        (HorizontalAlign::Start, "x"),
        (HorizontalAlign::Center, "  x"),
        (HorizontalAlign::End, "    x"),
    ] {
        let view = View::text("x")
            .width(WidthRule::Fill)
            .text_align(align)
            .into_view();
        let rows = compile_view(&view, 5).rows;
        assert_eq!(text(&rows[0]), expected);
    }
}

#[test]
fn ancestor_and_child_text_styles_cascade_to_physical_text() {
    let mut child = View::text("x").into_view();
    child.decoration.text_style = StyleSpec::new().foreground(ColorSpec::Ansi(2));
    let mut view = View::box_(child, Decoration::default());
    view.decoration.text_style = StyleSpec::new().foreground(ColorSpec::Ansi(1));

    let surface = ViewCompiler::default().layout(&view, 1, PhysicalStyle::default());
    assert_eq!(
        surface.get(0, 0).style.foreground,
        Some(PhysicalColor::Indexed(2))
    );
}

#[test]
fn span_style_overrides_node_and_explicit_false_cascades() {
    let mut child = View::styled_text(vec![
        TextSpan::plain("a"),
        TextSpan::styled("b", StyleSpec::new().bold()),
    ])
    .into_view();
    child.decoration.text_style = StyleSpec::new().attribute(TextAttribute::Bold, false);
    let mut view = View::box_(child, Decoration::default());
    view.decoration.text_style = StyleSpec::new().bold();

    let surface = ViewCompiler::default().layout(&view, 2, PhysicalStyle::default());
    assert!(!surface.get(0, 0).style.bold);
    assert!(surface.get(1, 0).style.bold);
}

#[test]
fn surface_background_paints_text_backing_and_transparent_tail() {
    let mut view = View::text("x").width(WidthRule::Fill).into_view();
    view.decoration.surface_background = Some(ColorSpec::Ansi(1));
    let surface = ViewCompiler::default().layout(&view, 4, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
    assert_eq!(
        surface.get(3, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
    assert!(surface.get(3, 0).painted);
}

#[test]
fn final_surface_background_api_paints_text_and_tail() {
    let view = View::text("x")
        .fill_width()
        .background(ColorSpec::ansi(1))
        .into_view();
    let surface = ViewCompiler::default().layout(&view, 4, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
    assert_eq!(
        surface.get(3, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn final_text_style_background_only_paints_text_cells() {
    let view = View::text("x")
        .fill_width()
        .style(StyleSpec::new().background(ColorSpec::ansi(1)))
        .into_view();
    let surface = ViewCompiler::default().layout(&view, 4, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
    assert_eq!(surface.get(3, 0).style.background, None);
}

#[test]
fn final_foreground_api_inherits_to_descendant_text() {
    let view = View::vertical(|column| {
        column.child("hello");
    })
    .foreground(ColorSpec::ansi(1));
    let surface = ViewCompiler::default().layout(&view, 5, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.foreground,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn final_attribute_api_supports_false_and_specific_child_override() {
    let inherited_bold_cancelled = View::vertical(|column| {
        column.child(View::text("x").text_attribute(TextAttribute::Bold, false));
    })
    .bold();
    let child_bold = View::vertical(|column| {
        column.child(View::text("x").bold());
    })
    .text_attribute(TextAttribute::Bold, false);
    let compiler = ViewCompiler::default();

    let cancelled = compiler.layout(&inherited_bold_cancelled, 1, PhysicalStyle::default());
    let overridden = compiler.layout(&child_bold, 1, PhysicalStyle::default());
    assert!(!cancelled.get(0, 0).style.bold);
    assert!(overridden.get(0, 0).style.bold);
}

#[test]
fn final_span_style_remains_more_specific_than_node_foreground() {
    let view = View::styled_text([
        TextSpan::plain("a"),
        TextSpan::styled("b", StyleSpec::new().foreground(ColorSpec::ansi(2))),
    ])
    .foreground(ColorSpec::ansi(1))
    .into_view();
    let surface = ViewCompiler::default().layout(&view, 2, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.foreground,
        Some(PhysicalColor::Indexed(1))
    );
    assert_eq!(
        surface.get(1, 0).style.foreground,
        Some(PhysicalColor::Indexed(2))
    );
}

#[test]
fn final_empty_properties_use_existing_geometry_rules() {
    let compiler = ViewCompiler::default();
    let empty = View::vertical(|_| {});
    let background = empty.clone().background(ColorSpec::ansi(1));
    let padding = empty.clone().padding(1);
    let border = empty.clone().border(BorderSpec::plain());
    let combined = empty
        .padding(1)
        .border(BorderSpec::plain())
        .background(ColorSpec::ansi(1));

    assert_block_shape(&compiler.compile(&background, 10), 0, 0);
    assert_block_shape(&compiler.compile(&padding, 10), 2, 2);
    assert_block_shape(&compiler.compile(&border, 10), 2, 2);
    assert_block_shape(&compiler.compile(&combined, 10), 4, 4);
}

#[test]
fn final_border_api_preserves_surface_background_and_border_color() {
    let view = View::vertical(|_| {})
        .background(ColorSpec::ansi(1))
        .border(BorderSpec::plain().color(ColorSpec::ansi(2)));
    let surface = ViewCompiler::default().layout(&view, 10, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.foreground,
        Some(PhysicalColor::Indexed(2))
    );
    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn final_structural_order_affects_clamp_geometry() {
    let padded_then_clamped = View::text("x")
        .padding(1)
        .clamp_rows(1, OverflowIndicator::None);
    let clamped_then_padded = View::text("x")
        .clamp_rows(1, OverflowIndicator::None)
        .padding(1);
    let compiler = ViewCompiler::default();

    assert_eq!(compiler.compile(&padded_then_clamped, 10).rows.len(), 1);
    assert_eq!(compiler.compile(&clamped_then_padded, 10).rows.len(), 3);
}

#[test]
fn undecorated_container_is_physical_identity_and_preserves_zero_width_height() {
    let compiler = ViewCompiler::default();
    for width in [0, 1, 5, 20] {
        let plain = View::text("x").into_view();
        let wrapped = plain.clone().container();
        let plain_block = compiler.compile(&plain, width);
        let wrapped_block = compiler.compile(&wrapped, width);
        assert_eq!(wrapped_block.width, plain_block.width);
        assert_eq!(wrapped_block.rows, plain_block.rows);
        assert_eq!(
            wrapped_block.physically_complete,
            plain_block.physically_complete
        );
    }

    let spacer = View::spacer(3).container();
    assert_block_shape(&compiler.compile(&spacer, 10), 0, 3);
}

#[test]
fn final_clamp_preserves_zero_width_vertical_extent() {
    let view = View::spacer(3).clamp_rows(4, OverflowIndicator::None);
    assert_block_shape(&ViewCompiler::default().compile(&view, 10), 0, 3);
}

#[test]
fn explicit_border_constructor_uses_rounded_glyphs() {
    let view = View::text("x").border(BorderSpec::rounded().color(ColorSpec::ansi(2)));
    let rows = ViewCompiler::default().compile(&view.into_view(), 5).rows;
    assert!(text(&rows[0]).starts_with('╭'));
    assert_eq!(
        rows[0].style_at(0).and_then(|style| style.foreground),
        Some(PhysicalColor::Indexed(2))
    );
}

#[test]
fn explicit_style_properties_merge_without_losing_fields() {
    let view = View::text("x")
        .foreground(ColorSpec::ansi(1))
        .bold()
        .style(StyleSpec::new().italic())
        .into_view();

    assert_eq!(
        view.decoration.text_style.foreground,
        Some(ColorSpec::ansi(1))
    );
    assert_eq!(view.decoration.text_style.attributes.bold, Some(true));
    assert_eq!(view.decoration.text_style.attributes.italic, Some(true));
}

#[test]
fn explicit_border_color_preserves_surface_background() {
    let mut decoration = Decoration::background(ColorSpec::Ansi(1));
    decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: Some(ColorSpec::Ansi(2)),
    });
    let view = View::box_(
        View::text("x").width(WidthRule::Fill).into_view(),
        decoration,
    )
    .width(WidthRule::Fill);
    let surface = ViewCompiler::default().layout(&view, 5, PhysicalStyle::default());

    let border = surface.get(0, 1).style;
    assert_eq!(border.foreground, Some(PhysicalColor::Indexed(2)));
    assert_eq!(border.background, Some(PhysicalColor::Indexed(1)));
}

#[test]
fn implicit_border_color_preserves_surface_background_and_inherits_foreground() {
    let mut decoration = Decoration::background(ColorSpec::Ansi(1));
    decoration.text_style = StyleSpec::new().foreground(ColorSpec::Ansi(2));
    decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: None,
    });
    let view = View::box_(
        View::text("x").width(WidthRule::Fill).into_view(),
        decoration,
    )
    .width(WidthRule::Fill);
    let surface = ViewCompiler::default().layout(&view, 5, PhysicalStyle::default());

    let border = surface.get(0, 1).style;
    assert_eq!(border.foreground, Some(PhysicalColor::Indexed(2)));
    assert_eq!(border.background, Some(PhysicalColor::Indexed(1)));
}

#[test]
fn text_background_does_not_leak_into_border() {
    let mut decoration = Decoration::default();
    decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
    decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: None,
    });
    let view = View::box_(
        View::text("x").width(WidthRule::Fill).into_view(),
        decoration,
    )
    .width(WidthRule::Fill);
    let surface = ViewCompiler::default().layout(&view, 5, PhysicalStyle::default());

    assert_eq!(
        surface.get(1, 1).style.background,
        Some(PhysicalColor::Indexed(2))
    );
    assert_eq!(surface.get(0, 1).style.background, None);
}

#[test]
fn surface_and_text_backgrounds_coexist_across_border_and_content() {
    let mut decoration = Decoration::background(ColorSpec::Ansi(1));
    decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
    decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: None,
    });
    let view = View::box_(
        View::text("x").width(WidthRule::Fill).into_view(),
        decoration,
    )
    .width(WidthRule::Fill);
    let surface = ViewCompiler::default().layout(&view, 5, PhysicalStyle::default());

    assert_eq!(
        surface.get(1, 1).style.background,
        Some(PhysicalColor::Indexed(2))
    );
    assert_eq!(
        surface.get(0, 1).style.background,
        Some(PhysicalColor::Indexed(1))
    );
    assert_eq!(
        surface.get(4, 1).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn border_painting_preserves_tiny_width_geometry() {
    let mut decoration = Decoration::background(ColorSpec::Ansi(1));
    decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: Some(ColorSpec::Ansi(2)),
    });
    let view = View::box_(View::text("x").into_view(), decoration).width(WidthRule::Fill);

    for width in [0, 1, 2, 3, 10] {
        let block = compile_view(&view, width);
        assert!(block.width <= width);
        assert!(
            block
                .rows
                .iter()
                .all(|row| row.width() <= usize::from(width))
        );
    }
}

#[test]
fn text_background_only_paints_text_cells() {
    let mut view = View::text("x").width(WidthRule::Fill).into_view();
    view.decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
    let surface = ViewCompiler::default().layout(&view, 4, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(2))
    );
    assert!(!surface.get(3, 0).painted);
}

#[test]
fn explicit_text_background_wins_over_surface_background() {
    let mut view = View::text("x").width(WidthRule::Fill).into_view();
    view.decoration.surface_background = Some(ColorSpec::Ansi(1));
    view.decoration.text_style = StyleSpec::new().background(ColorSpec::Ansi(2));
    let surface = ViewCompiler::default().layout(&view, 4, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(2))
    );
    assert_eq!(
        surface.get(3, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn nested_surface_backgrounds_preserve_child_region() {
    let child = View::box_(
        View::text("x").into_view(),
        Decoration::background(ColorSpec::Ansi(2)),
    );
    let outer =
        View::box_(child, Decoration::background(ColorSpec::Ansi(1))).width(WidthRule::Fill);
    let surface = ViewCompiler::default().layout(&outer, 4, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(2))
    );
    assert_eq!(
        surface.get(3, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn transparent_padding_shows_ancestor_surface_background() {
    let child = View::box_(
        View::text("x").into_view(),
        Decoration::default().padding(Insets::all(1)),
    );
    let outer =
        View::box_(child, Decoration::background(ColorSpec::Ansi(1))).width(WidthRule::Fill);
    let surface = ViewCompiler::default().layout(&outer, 5, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn surface_background_does_not_enter_text_style_cascade() {
    let mut view = View::text("x").into_view();
    view.decoration.surface_background = Some(ColorSpec::Ansi(1));
    let resolved = ViewCompiler::default()
        .theme
        .resolve_text_style(PhysicalStyle::default(), &view.decoration.text_style);
    assert_eq!(resolved.background, None);
}

#[test]
fn projected_egc_spans_exact_run_boundaries_and_history_ownership() {
    let compiler = ViewCompiler::default();
    let projected = ProjectedText {
        content_range: range(0, 12),
        terminator: ExactTerminator::None,
        width: WidthRule::Fit,
        wrap: WrapMode::WordThenGrapheme,
        align: HorizontalAlign::Start,
        layout: ProjectedTextLayout::Plain,
        runs: vec![
            ProjectedTextRun {
                display: "a".to_string(),
                style: style("markdown.bold"),
                owned: range(0, 3),
                exact_visible: Some(range(2, 3)),
            },
            ProjectedTextRun {
                display: "\u{301} rest".to_string(),
                style: style("text.default"),
                owned: range(3, 12),
                exact_visible: Some(range(5, 12)),
            },
        ],
    };
    let (_, rows) = compiler.compile_projected_text_with_metadata(&projected, 1);
    assert_eq!(text(&rows[0].row), "a\u{301}");
    assert_eq!(rows[0].source_end, Some(7));

    let view = StreamView::new(vec![StreamNode::projected_text(projected)]);
    let compiled = crate::presentation::stream::compile_stream(
        &view,
        1,
        crate::presentation::StreamOffset::new(12),
    );
    assert_eq!(
        compiled.commit[0],
        StreamRowCommit::Exact(StreamOffset::new(7))
    );
}

#[test]
fn projected_egc_spans_zwj_run_boundaries_without_splitting() {
    let first = "👩";
    let second = "\u{200d}💻";
    let split = first.len() as u64;
    let end = split + second.len() as u64;
    let projected = ProjectedText {
        content_range: range(0, end),
        terminator: ExactTerminator::None,
        width: WidthRule::Fit,
        wrap: WrapMode::WordThenGrapheme,
        align: HorizontalAlign::Start,
        layout: ProjectedTextLayout::Plain,
        runs: vec![
            ProjectedTextRun {
                display: first.to_string(),
                style: style("markdown.bold"),
                owned: range(0, split),
                exact_visible: Some(range(0, split)),
            },
            ProjectedTextRun {
                display: second.to_string(),
                style: style("markdown.italic"),
                owned: range(split, end),
                exact_visible: Some(range(split, end)),
            },
        ],
    };
    let (_, rows) = ViewCompiler::default().compile_projected_text_with_metadata(&projected, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(text(&rows[0].row), format!("{first}{second}"));
}

#[test]
fn projected_replacement_remains_one_indivisible_atom() {
    let projected = ProjectedText {
        content_range: range(0, 7),
        terminator: ExactTerminator::None,
        width: WidthRule::Fit,
        wrap: WrapMode::WordThenGrapheme,
        align: HorizontalAlign::Start,
        layout: ProjectedTextLayout::Plain,
        runs: vec![
            ProjectedTextRun {
                display: "foo".to_string(),
                style: StyleSpec::default(),
                owned: range(0, 3),
                exact_visible: Some(range(0, 3)),
            },
            ProjectedTextRun {
                display: "    ".to_string(),
                style: StyleSpec::default(),
                owned: range(3, 4),
                exact_visible: None,
            },
            ProjectedTextRun {
                display: "bar".to_string(),
                style: StyleSpec::default(),
                owned: range(4, 7),
                exact_visible: Some(range(4, 7)),
            },
        ],
    };
    let (_, rows) = ViewCompiler::default().compile_projected_text_with_metadata(&projected, 1);
    assert!(rows.iter().any(|row| text(&row.row) == "    "));
}

#[test]
fn projected_egc_boundaries_still_expose_independent_checkpoints() {
    let projected = ProjectedText {
        content_range: range(0, 2),
        terminator: ExactTerminator::None,
        width: WidthRule::Fit,
        wrap: WrapMode::WordThenGrapheme,
        align: HorizontalAlign::Start,
        layout: ProjectedTextLayout::Plain,
        runs: vec![
            ProjectedTextRun {
                display: "a".to_string(),
                style: style("markdown.bold"),
                owned: range(0, 1),
                exact_visible: Some(range(0, 1)),
            },
            ProjectedTextRun {
                display: "b".to_string(),
                style: style("markdown.italic"),
                owned: range(1, 2),
                exact_visible: Some(range(1, 2)),
            },
        ],
    };
    let compiled = crate::presentation::stream::compile_stream(
        &StreamView::new(vec![StreamNode::projected_text(projected)]),
        1,
        crate::presentation::StreamOffset::new(2),
    );
    assert_eq!(
        compiled.commit[0],
        StreamRowCommit::Exact(StreamOffset::new(1))
    );
    assert_eq!(
        compiled.commit[1],
        StreamRowCommit::Exact(StreamOffset::new(2))
    );
}

#[test]
fn default_decoration_keeps_core_tails_transparent() {
    let compiler = ViewCompiler::default();
    let views = [
        View::text("a").width(WidthRule::Fill).into_view(),
        View::column(vec![View::text("a").width(WidthRule::Fill).into_view()], 0),
        View::row(vec![RowChild::content(View::text("a").into_view())], 0),
        View::spacer(1).width(WidthRule::Fill),
        View::text("a")
            .fill_width()
            .clamp_rows(1, OverflowIndicator::None),
    ];

    for (index, view) in views.into_iter().enumerate() {
        let surface = compiler.layout(&view, 4, PhysicalStyle::default());
        if index == 3 {
            assert!(surface.cells.iter().all(|cell| !cell.painted));
        } else {
            assert!(surface.cells.iter().any(|cell| cell.painted));
            assert!(!surface.get(3, 0).painted);
        }
    }
}

#[test]
fn decorated_shell_paints_through_transparent_core() {
    let view = View::box_(
        View::spacer(1).width(WidthRule::Fill),
        Decoration::background(ColorSpec::Ansi(1)),
    );
    let surface = ViewCompiler::default().layout(&view, 3, PhysicalStyle::default());

    assert!(surface.get(0, 0).painted);
    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn explicit_child_paint_wins_over_outer_background() {
    let child = View::styled_text(vec![TextSpan::styled(
        "x",
        StyleSpec {
            background: Some(ColorSpec::Ansi(2)),
            ..StyleSpec::default()
        },
    )])
    .width(WidthRule::Fill)
    .into_view();
    let view = View::box_(child, Decoration::background(ColorSpec::Ansi(1)));
    let surface = ViewCompiler::default().layout(&view, 3, PhysicalStyle::default());

    assert_eq!(
        surface.get(0, 0).style.background,
        Some(PhysicalColor::Indexed(2))
    );
    assert_eq!(
        surface.get(2, 0).style.background,
        Some(PhysicalColor::Indexed(1))
    );
}

#[test]
fn decoration_preserves_physical_incompleteness() {
    let view = View::box_(
        View::text("漢").into_view(),
        Decoration::background(ColorSpec::Ansi(1)),
    );
    let compiler = ViewCompiler::default();

    assert!(!compiler.compile(&view, 1).physically_complete);
    assert!(compiler.compile(&view, 2).physically_complete);
}

#[test]
fn box_background_covers_padding_and_row_gap() {
    let view = View::box_(
        tool_view("body"),
        Decoration::background(ColorSpec::Theme(ThemeKey::from("surface.user")))
            .padding(Insets::all(1)),
    );
    let rows = compile_view(&view, 12).rows;
    assert!(rows.iter().all(|row| {
        row.cells()
            .iter()
            .any(|cell| cell.style.background.is_some())
    }));
}

#[test]
fn ordinary_view_does_not_partially_paint_wide_grapheme() {
    let compiler = ViewCompiler::default();
    let view = View::text("漢").into_view();

    let block = compiler.compile(&view, 1);

    // Whatever the established clipped representation is,
    // it must not contain a half-painted wide grapheme.
    for row in &block.rows {
        for cell in row.cells() {
            assert!(
                !cell
                    .grapheme
                    .as_deref()
                    .is_some_and(|text| text.contains('漢'))
            );
        }
    }
}

fn assert_block_shape(block: &LayoutBlock, width: u16, height: usize) {
    assert_eq!(block.width, width);
    assert_eq!(block.rows.len(), height);
}

fn empty_vertical() -> View {
    View::vertical(|_| {})
}

#[test]
fn width_defaults_and_explicit_fill_are_intrinsic_and_allocated() {
    let compiler = ViewCompiler::default();
    let horizontal = View::horizontal(|row| {
        row.child("ab");
        row.child("cde");
        row.gap(1);
    });
    let vertical = View::vertical(|column| {
        column.child("ab");
        column.child("abcde");
    });

    assert_block_shape(&compiler.compile(&horizontal, 10), 6, 1);
    assert_block_shape(&compiler.compile(&horizontal.fill_width(), 10), 10, 1);
    assert_block_shape(&compiler.compile(&vertical, 10), 5, 2);
    assert_block_shape(&compiler.compile(&vertical.fill_width(), 10), 10, 2);
}

#[test]
fn nested_fill_does_not_change_fit_child_allocation() {
    let compiler = ViewCompiler::default();
    let mut fit_child = View::text("x").fit_width().into_view();
    fit_child.decoration.surface_background = Some(ColorSpec::Ansi(1));
    let mut fill_child = View::text("x").fill_width().into_view();
    fill_child.decoration.surface_background = Some(ColorSpec::Ansi(1));

    let fit_parent = View::vertical(|column| {
        column.child(fit_child);
    })
    .fill_width();
    let fill_parent = View::vertical(|column| {
        column.child(fill_child);
    })
    .fill_width();
    let fit = compiler.compile(&fit_parent, 8);
    let fill = compiler.compile(&fill_parent, 8);

    assert_eq!(fit.width, 8);
    assert_eq!(text(&fit.rows[0]), "x");
    assert_eq!(fill.width, 8);
    assert_eq!(text(&fill.rows[0]), "x       ");
}

#[test]
fn spacer_has_zero_intrinsic_width_and_preserves_height() {
    let compiler = ViewCompiler::default();
    let fit = compiler.compile(&View::spacer(2), 10);
    let fill = compiler.compile(&View::spacer(2).fill_width(), 10);

    assert_block_shape(&fit, 0, 2);
    assert!(fit.physically_complete);
    assert!(
        fit.rows
            .iter()
            .all(|row| row.cells().iter().all(|cell| !cell.painted))
    );
    assert_block_shape(&fill, 10, 2);
    assert!(fill.physically_complete);
    assert!(
        fill.rows
            .iter()
            .all(|row| row.cells().iter().all(|cell| !cell.painted))
    );

    let zero_fit = compiler.compile(&View::spacer(0), 10);
    let zero_fill = compiler.compile(&View::spacer(0).fill_width(), 10);
    assert_block_shape(&zero_fit, 0, 0);
    assert_block_shape(&zero_fill, 10, 0);
}

#[test]
fn zero_width_spacers_contribute_vertical_extent_and_horizontal_height() {
    let compiler = ViewCompiler::default();
    let column = View::vertical(|column| {
        column.child("a");
        column.child(View::spacer(2));
        column.child("b");
    });
    let row = View::horizontal(|row| {
        row.child(View::spacer(3));
    });

    assert_block_shape(&compiler.compile(&column, 10), 1, 4);
    assert_block_shape(&compiler.compile(&row, 10), 0, 3);
}

#[test]
fn empty_flows_have_no_intrinsic_or_gap_geometry() {
    let compiler = ViewCompiler::default();
    let empty_horizontal = View::horizontal(|row| {
        row.gap(50);
    });
    let empty_vertical = View::vertical(|column| {
        column.gap(50);
    });
    let filled_horizontal = empty_horizontal.clone().fill_width();
    let filled_vertical = empty_vertical.clone().fill_width();

    assert_block_shape(&compiler.compile(&empty_horizontal, 10), 0, 0);
    assert_block_shape(&compiler.compile(&empty_vertical, 10), 0, 0);
    assert_block_shape(&compiler.compile(&filled_horizontal, 10), 10, 0);
    assert_block_shape(&compiler.compile(&filled_vertical, 10), 10, 0);
}

#[test]
fn one_child_gap_has_no_geometry() {
    let compiler = ViewCompiler::default();
    let vertical = View::vertical(|column| {
        column.child("x");
    });
    let vertical_gap = View::vertical(|column| {
        column.child("x");
        column.gap(50);
    });
    let horizontal = View::horizontal(|row| {
        row.child("x");
    });
    let horizontal_gap = View::horizontal(|row| {
        row.child("x");
        row.gap(50);
    });

    let vertical = compiler.compile(&vertical, 10);
    let vertical_gap = compiler.compile(&vertical_gap, 10);
    let horizontal = compiler.compile(&horizontal, 10);
    let horizontal_gap = compiler.compile(&horizontal_gap, 10);
    assert_eq!(vertical.width, vertical_gap.width);
    assert_eq!(vertical.rows, vertical_gap.rows);
    assert_eq!(horizontal.width, horizontal_gap.width);
    assert_eq!(horizontal.rows, horizontal_gap.rows);
}

#[test]
fn gaps_are_counted_between_all_semantic_children() {
    let compiler = ViewCompiler::default();
    let vertical = View::vertical(|column| {
        column.child("a");
        column.child("b");
        column.child("c");
        column.gap(2);
    });
    let horizontal = View::horizontal(|row| {
        row.child("a");
        row.child(View::spacer(1));
        row.child("c");
        row.gap(2);
    });

    assert_block_shape(&compiler.compile(&vertical, 10), 1, 7);
    assert_block_shape(&compiler.compile(&horizontal, 10), 6, 1);
}

#[test]
fn empty_background_does_not_create_geometry() {
    let compiler = ViewCompiler::default();
    let mut view = empty_vertical();
    view.decoration.surface_background = Some(ColorSpec::Ansi(1));
    let surface = compiler.layout(&view, 10, PhysicalStyle::default());

    assert_eq!((surface.width, surface.height), (0, 0));
    assert!(surface.cells.is_empty());
}

#[test]
fn empty_padding_creates_geometry_and_background_paints_it() {
    let compiler = ViewCompiler::default();
    let mut view = empty_vertical();
    view.decoration = Decoration::default().padding(Insets::all(1));
    view.decoration.surface_background = Some(ColorSpec::Ansi(1));
    let surface = compiler.layout(&view, 10, PhysicalStyle::default());

    assert_eq!((surface.width, surface.height), (2, 2));
    assert!(surface.cells.iter().all(|cell| cell.painted));
    assert!(
        surface
            .cells
            .iter()
            .all(|cell| cell.style.background == Some(PhysicalColor::Indexed(1)))
    );
}

#[test]
fn empty_border_geometry_is_safe_at_tiny_widths() {
    let compiler = ViewCompiler::default();
    let mut view = empty_vertical();
    view.decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: None,
    });
    let surface = compiler.layout(&view, 10, PhysicalStyle::default());
    assert_eq!((surface.width, surface.height), (2, 2));
    assert!(surface.cells.iter().all(|cell| cell.painted));

    for width in 0..=2 {
        let _ = compiler.compile(&view, width);
    }
}

#[test]
fn empty_padding_and_border_add_their_outer_geometry() {
    let compiler = ViewCompiler::default();
    let mut view = empty_vertical();
    view.decoration = Decoration::default().padding(Insets::all(1));
    view.decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: None,
    });
    let surface = compiler.layout(&view, 10, PhysicalStyle::default());

    assert_eq!((surface.width, surface.height), (4, 4));
}

#[test]
fn empty_border_and_background_compose_without_changing_geometry() {
    let compiler = ViewCompiler::default();
    let mut view = empty_vertical();
    view.decoration = Decoration::background(ColorSpec::Ansi(1));
    view.decoration.border = Some(BorderSpec {
        style: BorderStyle::Plain,
        color: None,
    });
    let surface = compiler.layout(&view, 10, PhysicalStyle::default());

    assert_eq!((surface.width, surface.height), (2, 2));
    assert!(
        surface
            .cells
            .iter()
            .all(|cell| cell.style.background == Some(PhysicalColor::Indexed(1)))
    );
}

#[test]
fn fixed_track_preserves_parent_width_and_child_sizing_intent() {
    let compiler = ViewCompiler::default();
    let mut fit_child = View::text("x").fit_width().into_view();
    fit_child.decoration.surface_background = Some(ColorSpec::Ansi(1));
    let mut fill_child = View::text("x").fill_width().into_view();
    fill_child.decoration.surface_background = Some(ColorSpec::Ansi(1));
    let fit = View::horizontal(|row| {
        row.fixed(5, fit_child);
    });
    let fill = View::horizontal(|row| {
        row.fixed(5, fill_child);
    });

    let fit = compiler.compile(&fit, 10);
    let fill = compiler.compile(&fill, 10);
    assert_eq!(fit.width, 5);
    assert_eq!(fill.width, 5);
    assert_eq!(text(&fit.rows[0]), "x");
    assert_eq!(text(&fill.rows[0]), "x    ");
}

#[test]
fn clamp_and_container_preserve_zero_width_vertical_extent() {
    let compiler = ViewCompiler::default();
    let spacer = View::spacer(3);
    let container = View::box_(spacer.clone(), Decoration::default());
    let clamped = spacer.clamp_rows(4, OverflowIndicator::None);

    assert_block_shape(&compiler.compile(&container, 10), 0, 3);
    assert_block_shape(&compiler.compile(&clamped, 10), 0, 3);
}

#[test]
fn clamp_zero_rows_remains_safe_for_empty_child() {
    let compiler = ViewCompiler::default();
    let view = View::vertical(|_| {}).clamp_rows(0, OverflowIndicator::None);
    assert_block_shape(&compiler.compile(&view, 10), 0, 0);
}

#[test]
fn clamp_emits_indicator() {
    let view = View::text("one two three four").clamp_rows(
        2,
        crate::presentation::api::style::OverflowIndicator::Ellipsis {
            style: StyleSpec::default(),
        },
    );
    let rows = compile_view(&view, 4).rows;
    assert_eq!(rows.len(), 2);
    assert!(text(&rows[1]).contains('…'));
}
