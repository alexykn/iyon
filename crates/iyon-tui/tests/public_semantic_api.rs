use iyon_tui::{
    BorderSpec, ColorSpec, Horizontal, HorizontalAlign, IntoView, StyleSpec, TextAttribute,
    TextSpan, Vertical, VerticalAlign, View, WrapMode,
};

struct Status(String);

impl IntoView for Status {
    fn into_view(self) -> View {
        View::text(self.0).bold().into_view()
    }
}

fn add_toolbar(row: &mut Horizontal) {
    row.child("left");
    row.flex(Status("ready".into()));
    row.fixed(6, "right");
    row.gap(1);
    row.vertical_align(VerticalAlign::Center);
}

fn add_details(column: &mut Vertical) {
    column.child("first");
    column.child("second");
    column.gap(1);
}

#[test]
fn public_semantic_composition_is_externally_usable() {
    let view = View::vertical(|column| {
        column.child(
            View::text("Title")
                .foreground(ColorSpec::theme("text.strong"))
                .bold()
                .padding(1),
        );
        column.child(
            View::horizontal(|row| {
                row.child(
                    View::styled_text([
                        TextSpan::plain("hello "),
                        TextSpan::styled("world", StyleSpec::new().italic()),
                    ])
                    .wrap(WrapMode::WordThenGrapheme)
                    .text_align(HorizontalAlign::Start),
                );
                add_toolbar(row);
            })
            .fill_width()
            .background(ColorSpec::theme("surface.panel"))
            .border(BorderSpec::rounded().color(ColorSpec::theme("border.muted"))),
        );
        add_details(column);
    });

    let _: View = view;
}

#[test]
fn public_text_properties_remain_typed_until_conversion() {
    let text = View::text("x")
        .padding(1)
        .background(ColorSpec::ansi(1))
        .foreground(ColorSpec::ansi(2))
        .border(BorderSpec::plain())
        .bold()
        .dim()
        .italic()
        .underline()
        .reversed()
        .text_attribute(TextAttribute::Bold, false)
        .style(StyleSpec::new().italic())
        .no_wrap()
        .text_align(HorizontalAlign::End)
        .fill_width();

    let _: View = text.clone().into_view();
    let _: View = text.container();
    let _: View = View::text("x").clamp_rows(1, iyon_tui::OverflowIndicator::None);
}

#[test]
fn public_into_view_accepts_owned_and_borrowed_values() {
    let source = String::from("owned");
    let _: View = source.as_str().into_view();
    let _: View = (&source).into_view();
    let _: View = source.into_view();

    let _: View = View::vertical(|column| {
        column.child(Status("status".into()));
    });
}
