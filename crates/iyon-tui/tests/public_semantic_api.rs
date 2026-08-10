use iyon_tui::{
    BorderSpec, ColorSpec, Component, ComponentCx, ComponentHandle, EventCx, Horizontal,
    HorizontalAlign, InteractionResult, IntoView, Key, KeyStroke, Modifiers, Output, OutputRouter,
    StyleSpec, TextAttribute, TextChange, TextInput, TextSpan, Vertical, VerticalAlign, View,
    WrapMode,
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

fn add_bounded_details(column: &mut Vertical) {
    column.fixed(2, View::text("header").fill_height());
    column.flex(View::text("body").fill_height());
}

fn hanging_component_view<C: Component>(handle: ComponentHandle<C>) -> View {
    View::hanging(
        View::text("> ").no_wrap(),
        View::text("  ").no_wrap(),
        View::component(handle).fill_width(),
    )
    .fill_width()
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
fn public_hanging_composition_is_externally_usable() {
    let view = View::hanging(
        View::text("> ").no_wrap(),
        View::text("  ").no_wrap(),
        View::text("body that wraps").fill_width(),
    )
    .fill_width();
    let _: View = view;
    let _: fn(ComponentHandle<Counter>) -> View = hanging_component_view::<Counter>;
}

#[test]
fn public_height_composition_is_externally_usable() {
    let view = View::vertical(add_bounded_details)
        .fill_width()
        .fill_height();
    let _: View = view;

    let text = View::text("x").fill_width().fill_height().bold();
    let _: View = text.into_view();
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

struct Counter {
    value: usize,
    changed: Output<usize>,
}

enum CounterCommand {
    Increment,
}

impl Counter {
    fn command(&self, key: KeyStroke) -> Option<CounterCommand> {
        (key.key() == Key::Char('+')).then_some(CounterCommand::Increment)
    }

    fn handle(&mut self, command: CounterCommand, cx: &mut EventCx<'_>) -> InteractionResult {
        match command {
            CounterCommand::Increment => {
                self.value += 1;
                cx.emit(self.changed, self.value);
                InteractionResult::Consumed
            }
        }
    }
}

impl Component for Counter {
    fn view(&self) -> View {
        View::text(self.value.to_string()).into_view()
    }

    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        cx.focusable();
        cx.key_commands(Self::command, Self::handle);
    }
}

fn component_view<C>(handle: ComponentHandle<C>) -> View {
    View::component(handle)
}

#[test]
fn public_component_and_output_contract_is_backend_free() {
    fn assert_component<C: Component>() {}
    assert_component::<Counter>();

    let output = Output::<usize>::new();
    let mut router = OutputRouter::new();
    router.route(output, |value| value + 1).unwrap();
    assert!(router.remove(output));

    let stroke = KeyStroke::with_modifiers(Key::Char('+'), Modifiers::CONTROL);
    assert_eq!(stroke.key(), Key::Char('+'));
    assert!(stroke.modifiers().contains(Modifiers::CONTROL));

    let _: Option<ComponentHandle<Counter>> = None;
    let _ = component_view::<Counter>;
}

#[test]
fn public_text_input_contract_is_backend_free() {
    fn assert_component<C: Component>() {}
    assert_component::<TextInput>();

    let mut input = TextInput::new().multiline(true);
    let submitted: Output<String> = input.submitted();
    let changed: Output<usize> =
        input.output_on_change(|change: TextChange<'_>| change.text().len());
    let _: fn(ComponentHandle<TextInput>) -> View = component_view::<TextInput>;
    let _: (Output<String>, Output<usize>) = (submitted, changed);
}
