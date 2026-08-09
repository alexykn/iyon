#[test]
fn exact_fragments_share_one_egc_barrier() {
    let text = ProjectedText {
        content_range: StreamRange::new(StreamOffset::ZERO, StreamOffset::new(3)),
        terminator: ExactTerminator::None,
        width: WidthRule::Fit,
        wrap: WrapMode::WordThenGrapheme,
        align: HorizontalAlign::Start,
        layout: ProjectedTextLayout::Plain,
        runs: vec![
            ProjectedTextRun {
                display: "e".into(),
                style: StyleSpec::default(),
                owned: StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1)),
                exact_visible: Some(StreamRange::new(StreamOffset::ZERO, StreamOffset::new(1))),
            },
            ProjectedTextRun {
                display: "\u{301}".into(),
                style: StyleSpec::default(),
                owned: StreamRange::new(StreamOffset::new(1), StreamOffset::new(3)),
                exact_visible: Some(StreamRange::new(StreamOffset::new(1), StreamOffset::new(3))),
            },
        ],
    };
    assert!(
        std::panic::catch_unwind(|| {
            StreamView::new(vec![StreamNode::projected_text(text)])
                .suffix_from(StreamOffset::new(1));
        })
        .is_err()
    );
}

use unicode_segmentation::UnicodeSegmentation;

use crate::physical::PhysicalRow;
use crate::presentation::layout::ViewCompiler;
use crate::presentation::{HorizontalAlign, StyleSpec, ThemeKey, WidthRule, WrapMode};
use crate::stream::*;

fn range(start: u64, end: u64) -> StreamRange {
    StreamRange::new(StreamOffset::new(start), StreamOffset::new(end))
}

fn text(row: &PhysicalRow) -> String {
    row.plain_text()
}

fn style(color: &str) -> StyleSpec {
    StyleSpec {
        foreground: Some(crate::ColorSpec::Theme(ThemeKey::from(color))),
        ..StyleSpec::default()
    }
}

fn hanging_text(
    content_range: StreamRange,
    prefix_source: StreamRange,
    show_prefix: bool,
) -> ProjectedText {
    ProjectedText {
        content_range,
        terminator: ExactTerminator::None,
        width: WidthRule::Fill,
        wrap: WrapMode::WordThenGrapheme,
        align: HorizontalAlign::Start,
        layout: ProjectedTextLayout::Hanging {
            body_column: 2,
            prefix: "- ".to_string(),
            prefix_style: StyleSpec::default(),
            prefix_source,
            show_prefix,
        },
        runs: vec![ProjectedTextRun {
            display: "item".to_string(),
            style: StyleSpec::default(),
            owned: StreamRange::new(prefix_source.end, content_range.end),
            exact_visible: Some(StreamRange::new(prefix_source.end, content_range.end)),
        }],
    }
}

fn snapshot(text: ProjectedText) -> StreamSnapshot {
    let range = text.owned_range();
    StreamSnapshot {
        revision: StreamRevision::ZERO,
        source_base: range.start,
        source_end: range.end,
        stable_through: range.end,
        view: StreamView::new(vec![StreamNode::projected_text(text)]),
    }
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
    let compiled = crate::stream::compile_stream(&view, 1, crate::stream::StreamOffset::new(12));
    assert_eq!(
        compiled.transfer[0],
        StreamRowTransfer::Checkpoint(StreamOffset::new(7))
    );
}

#[test]
fn projected_egc_spans_zwj_run_boundaries_without_splitting() {
    let first = "👩";
    let second = "\u{200d}💻x";
    let split = first.len() as u64;
    let zwj_boundary = split + "\u{200d}".len() as u64;
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
    let (_, rows) = ViewCompiler::default().compile_projected_text_with_metadata(&projected, 20);
    assert_eq!(rows.len(), 1);
    assert_eq!(text(&rows[0].row), format!("{first}{second}"));
    assert!(
        second
            .grapheme_indices(true)
            .any(|(start, _)| start == "\u{200d}".len())
    );
    assert!(!projected_checkpoint_is_legal(
        &projected,
        StreamOffset::new(zwj_boundary)
    ));
    assert!(
        std::panic::catch_unwind(|| {
            StreamView::new(vec![StreamNode::projected_text(projected)])
                .suffix_from(StreamOffset::new(zwj_boundary));
        })
        .is_err()
    );
}

#[test]
fn hanging_initial_and_suffix_nodes_validate_and_compile() {
    let initial = hanging_text(range(0, 6), range(0, 2), true);
    assert!(snapshot(initial.clone()).validate().is_ok());

    let suffix = StreamView::new(vec![StreamNode::projected_text(initial)])
        .suffix_from(StreamOffset::new(2));
    let StreamNode::Text(suffix_text) = &suffix.nodes[0] else {
        panic!("expected projected suffix");
    };
    let ProjectedTextLayout::Hanging {
        prefix_source,
        show_prefix,
        ..
    } = &suffix_text.layout
    else {
        panic!("expected hanging suffix");
    };
    assert_eq!(*prefix_source, range(0, 2));
    assert!(!show_prefix);
    assert!(snapshot(suffix_text.clone()).validate().is_ok());

    let compiled = crate::stream::compile_stream(&suffix, 20, StreamOffset::new(6));
    assert_eq!(compiled.rows[0].plain_text(), "  item");
}

#[test]
fn hanging_suffix_inside_prefix_source_is_rejected() {
    let text = hanging_text(range(0, 6), range(0, 2), true);
    let result = std::panic::catch_unwind(|| {
        StreamView::new(vec![StreamNode::projected_text(text)]).suffix_from(StreamOffset::new(1));
    });
    assert!(result.is_err());
}

#[test]
fn list_continuation_empty_prefix_at_content_start_validates() {
    let text = hanging_text(range(4, 8), range(4, 4), false);
    assert!(snapshot(text).validate().is_ok());
}

#[test]
fn hanging_width_diagnostic_is_reachable_and_structural_errors_are_distinct() {
    let mut text = hanging_text(range(0, 6), range(0, 2), true);
    let ProjectedTextLayout::Hanging { body_column, .. } = &mut text.layout else {
        unreachable!();
    };
    *body_column = 3;
    assert_eq!(
        snapshot(text).validate(),
        Err(StreamValidationError::Projected(
            ProjectedValidationError::HangingWidthMismatch
        ))
    );
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
    let compiled = crate::stream::compile_stream(
        &StreamView::new(vec![StreamNode::projected_text(projected)]),
        1,
        crate::stream::StreamOffset::new(2),
    );
    assert_eq!(
        compiled.transfer[0],
        StreamRowTransfer::Checkpoint(StreamOffset::new(1))
    );
    assert_eq!(
        compiled.transfer[1],
        StreamRowTransfer::Checkpoint(StreamOffset::new(2))
    );
}
