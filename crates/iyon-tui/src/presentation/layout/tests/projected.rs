use super::*;

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
