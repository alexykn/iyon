use super::*;
use crate::physical::PhysicalStyle;
use crate::presentation::{IntoView, layout::compile_view};
use crate::{Component, View};

fn focused_view(text: &str) -> crate::presentation::View {
    let mut input = TextInput::new().multiline(true);
    input.set_text(text);
    input.focused = true;
    input.view()
}

fn has_reversed(row: &crate::physical::PhysicalRow) -> bool {
    row.cells().iter().any(|cell| cell.style.reversed)
}

#[test]
fn focused_empty_and_trailing_cursors_are_visible_reversed_cells() {
    let empty = compile_view(&focused_view(""), 20);
    assert_eq!(empty.rows.len(), 1);
    assert!(has_reversed(&empty.rows[0]));

    let trailing = compile_view(&focused_view("abc"), 20);
    assert_eq!(trailing.rows[0].cells().len(), 4);
    assert!(trailing.rows[0].cells()[3].style.reversed);
}

#[test]
fn newline_cursor_uses_the_correct_logical_line() {
    let mut before = TextInput::new().multiline(true);
    before.set_text("a\nb");
    before.set_cursor_for_test(1);
    before.focused = true;
    let before = compile_view(&before.view(), 20);
    assert!(before.rows[0].cells()[1].style.reversed);
    assert!(!has_reversed(&before.rows[1]));

    let after = compile_view(&focused_view("a\n"), 20);
    assert!(!has_reversed(&after.rows[0]));
    assert!(has_reversed(&after.rows[1]));
}

#[test]
fn wide_and_combining_graphemes_are_not_split_by_cursor_style() {
    let mut wide_input = TextInput::new().multiline(true);
    wide_input.set_text("界");
    wide_input.set_cursor_for_test(0);
    wide_input.focused = true;
    let wide = compile_view(&wide_input.view(), 20);
    assert_eq!(wide.rows[0].cells().len(), 2);
    assert!(wide.rows[0].cells().iter().all(|cell| cell.style.reversed));

    let mut combining_input = TextInput::new().multiline(true);
    combining_input.set_text("e\u{301}");
    combining_input.set_cursor_for_test(0);
    combining_input.focused = true;
    let combining = compile_view(&combining_input.view(), 20);
    assert_eq!(combining.rows[0].cells().len(), 1);
    assert!(combining.rows[0].cells()[0].style.reversed);
}

#[test]
#[should_panic(expected = "text cursor anchor is not a UTF-8 boundary")]
fn invalid_cursor_anchor_fails_loudly() {
    let view = View::text("é").cursor_at(1).into_view();
    let _ = compile_view(&view, 20);
}

#[test]
fn unfocused_text_matches_ordinary_semantic_text() {
    let input = TextInput::new();
    let input_rows = compile_view(&input.view(), 20).rows;
    let ordinary_rows = compile_view(&View::text("").into_view(), 20).rows;
    assert_eq!(input_rows, ordinary_rows);
    assert!(
        input_rows
            .iter()
            .flat_map(|row| row.cells())
            .all(|cell| cell.style == PhysicalStyle::default())
    );
}
