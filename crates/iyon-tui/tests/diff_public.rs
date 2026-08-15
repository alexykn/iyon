use iyon_tui::{DiffHunk, DiffLine, DiffLineNumber, DiffRange, DiffRenderer, Renderer};

#[test]
fn structured_diff_can_be_constructed_and_rendered_from_the_crate_root() {
    let number = |line| DiffLineNumber::new(line).unwrap();
    let old = DiffRange::new(iyon_tui::DiffLineOffset::new(0), 2).unwrap();
    let new_side = DiffRange::new(iyon_tui::DiffLineOffset::new(0), 2).unwrap();
    let hunk = DiffHunk::new(
        old,
        new_side,
        [
            DiffLine::context(number(1), number(1), "same"),
            DiffLine::deletion(number(2), "removed")
                .with_termination(iyon_tui::DiffLineTermination::Unterminated),
            DiffLine::addition(number(2), "added"),
        ],
    )
    .unwrap();

    let _view = DiffRenderer::new().render(&hunk);
}
