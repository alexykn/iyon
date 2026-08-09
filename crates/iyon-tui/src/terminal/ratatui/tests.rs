use super::*;
use crate::{
    IntoView, View,
    physical::{AnsiColor, PhysicalCell, PhysicalColor, PhysicalRow, PhysicalStyle},
    presentation::layout::compile_view,
};
use ratatui::style::{Color, Modifier, Style};

#[test]
fn physical_style_lowers_without_quantization() {
    let style = PhysicalStyle {
        foreground: Some(PhysicalColor::Indexed(4)),
        background: Some(PhysicalColor::Rgb { r: 1, g: 2, b: 3 }),
        bold: true,
        dim: true,
        italic: true,
        underline: true,
        reversed: true,
    };
    let lowered = style::style(style);

    assert_eq!(lowered.fg, Some(Color::Indexed(4)));
    assert_eq!(lowered.bg, Some(Color::Rgb(1, 2, 3)));
    assert!(lowered.add_modifier.contains(Modifier::BOLD));
    assert!(lowered.add_modifier.contains(Modifier::DIM));
    assert!(lowered.add_modifier.contains(Modifier::ITALIC));
    assert!(lowered.add_modifier.contains(Modifier::UNDERLINED));
    assert!(lowered.add_modifier.contains(Modifier::REVERSED));

    assert_eq!(
        style::style(PhysicalStyle {
            foreground: Some(PhysicalColor::Named(AnsiColor::Red)),
            ..PhysicalStyle::default()
        })
        .fg,
        Some(Color::Red)
    );
    assert_eq!(
        style::physical_style(Style::default().fg(Color::Yellow)).foreground,
        Some(PhysicalColor::Named(AnsiColor::Yellow))
    );
}

#[test]
fn transparent_and_painted_rows_lower_differently() {
    assert_eq!(
        row_to_line(&PhysicalRow::empty()),
        ratatui::text::Line::from("")
    );

    let painted = PhysicalRow::from_cells(vec![
        PhysicalCell::blank(PhysicalStyle {
            background: Some(PhysicalColor::Indexed(1)),
            ..PhysicalStyle::default()
        }),
        PhysicalCell::blank(PhysicalStyle {
            background: Some(PhysicalColor::Indexed(1)),
            ..PhysicalStyle::default()
        }),
    ]);
    let line = row_to_line(&painted);
    assert_eq!(line.spans[0].content, "  ");
    assert_eq!(line.spans[0].style.bg, Some(Color::Indexed(1)));
}

#[test]
fn wide_continuation_is_emitted_once_and_styles_are_coalesced() {
    let style = PhysicalStyle {
        foreground: Some(PhysicalColor::Indexed(2)),
        ..PhysicalStyle::default()
    };
    let row = PhysicalRow::from_cells(vec![
        PhysicalCell {
            grapheme: Some("界".to_string()),
            style,
            painted: true,
            continuation: false,
        },
        PhysicalCell {
            grapheme: None,
            style,
            painted: true,
            continuation: true,
        },
        PhysicalCell {
            grapheme: Some("!".to_string()),
            style,
            painted: true,
            continuation: false,
        },
    ]);
    let line = row_to_line(&row);

    assert_eq!(line.spans.len(), 1);
    assert_eq!(line.spans[0].content, "界!");
}

#[test]
fn semantic_view_lowers_through_the_adapter() {
    let view = View::text("界")
        .fill_width()
        .background(crate::ColorSpec::ansi(1))
        .border(crate::BorderSpec::plain())
        .into_view();
    let block = compile_view(&view, 5);
    let lines = rows_to_lines(&block.rows);

    assert!(
        lines
            .iter()
            .any(|line| line.spans.iter().any(|span| span.content.contains('┌')))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.spans.iter().any(|span| span.content.contains('界')))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.spans.iter().any(|span| span.style.bg.is_some()))
    );
}
