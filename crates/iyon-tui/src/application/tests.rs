use std::{
    cell::{Cell, RefCell},
    convert::Infallible,
    rc::Rc,
    time::{Duration, Instant},
};

use super::{
    App, AppCx,
    kernel::{KernelError, RunningApp},
    timer::TimerQueue,
};
use crate::{
    BorderSpec, Component, ComponentCx, ComponentHandle, EventCx, History, HistoryError,
    InteractionResult, IntoView, Key, KeyStroke, Output, RouteConflict, TextInput, View,
    backend::NativeHistorySink, geometry::Size, physical::PhysicalRow,
};

#[derive(Debug)]
enum TestError {
    History,
    Route,
}

impl From<HistoryError> for TestError {
    fn from(error: HistoryError) -> Self {
        let _ = error;
        Self::History
    }
}

impl From<RouteConflict> for TestError {
    fn from(error: RouteConflict) -> Self {
        let _ = error;
        Self::Route
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Submit(String),
    Timer,
    First,
    Second,
    A,
    B,
    C,
    Loop,
    Tick,
    Exit,
    AfterExit,
    Mutate,
    Remove,
}

struct State {
    input: ComponentHandle<TextInput>,
    submitted: Vec<String>,
    count: usize,
    removed: bool,
    ticking: Option<ComponentHandle<Ticking>>,
}

fn body(state: &State) -> View {
    View::vertical(|column| {
        column.child(View::text(format!("count: {}", state.count)));
        column.child(View::component(state.input));
        if let Some(ticking) = state.ticking {
            column.child(View::component(ticking));
        }
    })
}

fn start<State, Action, Error, Init, Update, ViewFn>(
    app: App<State, Action, Error, Init, Update, ViewFn>,
    now: Instant,
) -> RunningApp<State, Action, Error, Update, ViewFn>
where
    Init: FnOnce(&mut AppCx<'_, Action>) -> Result<State, Error>,
    Update: FnMut(&mut State, Action, &mut AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> View,
    Error: std::fmt::Debug,
{
    app.start(now).expect("test application starts")
}

#[derive(Default)]
struct HeadlessSink {
    rows: Vec<PhysicalRow>,
}

impl NativeHistorySink for HeadlessSink {
    type Error = Infallible;

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
        self.rows.extend(rows.iter().cloned());
        Ok(rows.len())
    }
}

fn prepare<State, Action, Error, Update, ViewFn>(
    app: &mut RunningApp<State, Action, Error, Update, ViewFn>,
    now: Instant,
) where
    Update: FnMut(&mut State, Action, &mut AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> View,
{
    let mut sink = HeadlessSink::default();
    prepare_with_sink(app, &mut sink, now);
}

fn prepare_with_sink<State, Action, Error, Update, ViewFn>(
    app: &mut RunningApp<State, Action, Error, Update, ViewFn>,
    sink: &mut HeadlessSink,
    now: Instant,
) where
    Update: FnMut(&mut State, Action, &mut AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> View,
{
    app.prepare_frame(now, sink, |_| Ok(Size::new(40, 8)))
        .expect("headless frame prepares");
}

fn surface_text(frame: &crate::scene::PreparedSceneFrame) -> String {
    let mut text = String::new();
    for y in 0..frame.surface.height() {
        for x in 0..frame.surface.width() {
            if let Some(grapheme) = &frame.surface.get(x, y).grapheme {
                text.push_str(grapheme);
            }
        }
    }
    text
}

#[test]
fn neutral_app_composes_input_output_action_timer_and_persistent_history() {
    let now = Instant::now();
    let mut history = History::new();
    for index in 0..20 {
        history.push(format!("history-{index}")).unwrap();
    }
    let view_calls = Rc::new(Cell::new(0));

    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            let input = cx.register(TextInput::new().border(BorderSpec::plain()));
            let submitted = cx
                .with_component(input, TextInput::submitted)
                .expect("registered input");
            cx.route(submitted, Action::Submit)?;
            cx.schedule_after(Duration::from_millis(100), Action::Timer);
            Ok(State {
                input,
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        |state, action, cx| {
            match action {
                Action::Submit(text) => {
                    state.submitted.push(text.clone());
                    cx.history_mut().expect("configured history").push(text)?;
                }
                Action::Timer => state.count += 1,
                _ => {}
            }
            Ok(())
        },
        {
            let view_calls = Rc::clone(&view_calls);
            move |state: &State| {
                view_calls.set(view_calls.get() + 1);
                body(state)
            }
        },
    )
    .with_history(history);

    let mut app = start(app, now);
    let mut sink = HeadlessSink::default();
    assert_eq!(view_calls.get(), 1);
    prepare_with_sink(&mut app, &mut sink, now);
    let promoted_rows = sink.rows.len();
    assert!(promoted_rows > 0);
    assert_eq!(app.mount_count_for_test(), 1);
    assert_eq!(app.focusable_count_for_test(), 1);
    assert!(app.focused_for_test());

    assert_eq!(
        app.dispatch_key(KeyStroke::new(Key::Tab)).unwrap(),
        InteractionResult::Consumed
    );
    for key in ['h', 'i'] {
        assert_eq!(
            app.dispatch_key(KeyStroke::new(Key::Char(key))).unwrap(),
            InteractionResult::Consumed
        );
    }
    assert_eq!(
        app.dispatch_key(KeyStroke::new(Key::Enter)).unwrap(),
        InteractionResult::Consumed
    );
    let status = app.advance_ready(now).unwrap();
    assert!(status.dirty);
    assert_eq!(app.state.submitted, ["hi"]);
    let before_frame = view_calls.get();
    prepare_with_sink(&mut app, &mut sink, now);
    assert_eq!(view_calls.get(), before_frame + 1);
    assert!(sink.rows.len() >= promoted_rows);

    let timer_status = app.advance_ready(now + Duration::from_millis(99)).unwrap();
    assert!(!timer_status.dirty);
    let timer_status = app.advance_ready(now + Duration::from_millis(100)).unwrap();
    assert!(timer_status.dirty);
    assert_eq!(app.state.count, 1);

    let frame = app
        .prepare_frame(now + Duration::from_millis(100), &mut sink, |_| {
            Ok(Size::new(40, 8))
        })
        .unwrap();
    let text = surface_text(&frame);
    assert!(text.contains("hi"));
    assert!(
        sink.rows
            .iter()
            .any(|row| row.plain_text().contains("history-0"))
    );
}

#[test]
fn headless_paste_dispatch_stays_on_the_local_component_path() {
    let now = Instant::now();
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            Ok(State {
                input: cx.register(TextInput::new().border(BorderSpec::plain())),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        |_state, _action, _cx| Ok(()),
        |state: &State| View::component(state.input),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    assert_eq!(
        app.dispatch_paste("typed").unwrap(),
        InteractionResult::Consumed
    );
}

#[test]
fn queued_actions_are_fifo_and_coalesce_one_view_per_frame() {
    let now = Instant::now();
    let view_calls = Rc::new(Cell::new(0));
    let update_log = Rc::new(RefCell::new(Vec::new()));
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            cx.schedule_after(Duration::ZERO, Action::A);
            cx.schedule_after(Duration::ZERO, Action::B);
            cx.schedule_after(Duration::ZERO, Action::C);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        {
            let update_log = Rc::clone(&update_log);
            move |_state, action, _cx| {
                update_log.borrow_mut().push(action);
                Ok(())
            }
        },
        {
            let view_calls = Rc::clone(&view_calls);
            move |_state: &State| {
                view_calls.set(view_calls.get() + 1);
                View::text("body").into_view()
            }
        },
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    let before = view_calls.get();
    let status = app.advance_ready(now).unwrap();
    assert!(!status.more_ready);
    assert_eq!(*update_log.borrow(), [Action::A, Action::B, Action::C]);
    assert_eq!(view_calls.get(), before);
    prepare(&mut app, now);
    assert_eq!(view_calls.get(), before + 1);
}

#[test]
fn zero_duration_timer_is_queued_after_the_current_update() {
    let now = Instant::now();
    let log = Rc::new(RefCell::new(Vec::new()));
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            cx.schedule_after(Duration::ZERO, Action::First);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        {
            let log = Rc::clone(&log);
            move |_state, action, cx| {
                match action {
                    Action::First => {
                        log.borrow_mut().push("first-start");
                        cx.schedule_after(Duration::ZERO, Action::Second);
                        log.borrow_mut().push("first-end");
                    }
                    Action::Second => log.borrow_mut().push("second-start"),
                    _ => {}
                }
                Ok(())
            }
        },
        |_state: &State| View::text("body").into_view(),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    app.advance_ready(now).unwrap();
    assert_eq!(*log.borrow(), ["first-start", "first-end", "second-start"]);
}

#[test]
fn finite_batch_yields_a_self_rescheduling_zero_timer() {
    let now = Instant::now();
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            cx.schedule_after(Duration::ZERO, Action::Loop);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        |state, action, cx| {
            if action == Action::Loop {
                state.count += 1;
                if state.count < 300 {
                    cx.schedule_after(Duration::ZERO, Action::Loop);
                }
            }
            Ok(())
        },
        |_state: &State| View::text("body").into_view(),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    let first = app.advance_ready(now).unwrap();
    assert!(first.more_ready);
    assert_eq!(app.state.count, 128);
    prepare(&mut app, now);
    let second = app.advance_ready(now).unwrap();
    assert!(second.more_ready);
    assert_eq!(app.state.count, 256);
    prepare(&mut app, now);
    let third = app.advance_ready(now).unwrap();
    assert!(!third.more_ready);
    assert_eq!(app.state.count, 300);
}

#[test]
fn timers_are_ordered_cancelable_and_non_dirty_before_delivery() {
    let now = Instant::now();
    let mut timers = TimerQueue::default();
    let first = timers.schedule(now, Duration::from_millis(10), NonClone("first"));
    let second = timers.schedule(now, Duration::from_millis(10), NonClone("second"));
    let third = timers.schedule(now, Duration::from_millis(5), NonClone("third"));
    assert_eq!(timers.next_deadline(), Some(now + Duration::from_millis(5)));
    assert!(timers.cancel(second));
    assert!(!timers.cancel(second));
    assert_eq!(
        timers
            .pop_due(now + Duration::from_millis(5))
            .map(|value| value.0),
        Some("third")
    );
    assert_eq!(
        timers
            .pop_due(now + Duration::from_millis(10))
            .map(|value| value.0),
        Some("first")
    );
    assert!(timers.pop_due(now + Duration::from_millis(10)).is_none());
    assert!(!timers.cancel(third));
    let _ = first;

    let mut same_deadline = TimerQueue::default();
    same_deadline.schedule(now, Duration::ZERO, NonClone("a"));
    same_deadline.schedule(now, Duration::ZERO, NonClone("b"));
    assert_eq!(same_deadline.pop_due(now).map(|value| value.0), Some("a"));
    assert_eq!(same_deadline.pop_due(now).map(|value| value.0), Some("b"));

    let mut queue_a = TimerQueue::default();
    let mut queue_b = TimerQueue::default();
    let handle_a = queue_a.schedule(now, Duration::from_secs(1), NonClone("a"));
    let handle_b = queue_b.schedule(now, Duration::from_secs(1), NonClone("b"));
    assert_ne!(handle_a, handle_b);
    assert!(!queue_b.cancel(handle_a));
    assert!(!queue_a.cancel(handle_b));
    assert!(queue_a.cancel(handle_a));
    assert!(queue_b.cancel(handle_b));

    let fired = queue_a.schedule(now, Duration::ZERO, NonClone("fired"));
    assert_eq!(queue_a.pop_due(now).map(|value| value.0), Some("fired"));
    let replacement = queue_a.schedule(now, Duration::ZERO, NonClone("replacement"));
    assert_ne!(fired, replacement);
    assert!(!queue_a.cancel(fired));
    assert!(queue_a.cancel(replacement));
}

struct NonClone(&'static str);

#[test]
fn deadline_arbitration_chooses_timer_or_mounted_tick() {
    let now = Instant::now();
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            let ticking = cx.register(Ticking::new());
            cx.schedule_after(Duration::from_millis(50), Action::Timer);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: Some(ticking),
            })
        },
        |_state, _action, _cx| Ok(()),
        body,
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    assert_eq!(app.next_deadline(), Some(now + Duration::from_millis(50)));

    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            let ticking = cx.register(Ticking::new());
            cx.schedule_after(Duration::from_millis(200), Action::Timer);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: Some(ticking),
            })
        },
        |_state, _action, _cx| Ok(()),
        body,
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    assert_eq!(app.next_deadline(), Some(now + Duration::from_millis(80)));
}

#[derive(Debug)]
struct Ticking {
    output: Output<()>,
}

impl Ticking {
    fn new() -> Self {
        Self {
            output: Output::new(),
        }
    }

    fn tick(&mut self, _now: Instant, cx: &mut EventCx<'_>) -> bool {
        cx.emit(self.output, ());
        false
    }
}

impl Component for Ticking {
    fn view(&self) -> View {
        View::text("tick").into_view()
    }

    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        cx.tick(Duration::from_millis(80), Ticking::tick);
    }
}

#[test]
fn mounted_tick_output_is_routed_into_the_same_action_queue() {
    let now = Instant::now();
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            let ticking = cx.register(Ticking::new());
            let output = cx
                .with_component(ticking, |component| component.output)
                .expect("registered ticking component");
            cx.route(output, |_| Action::Tick)?;
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: Some(ticking),
            })
        },
        |state, action, _cx| {
            if action == Action::Tick {
                state.count += 1;
            }
            Ok(())
        },
        body,
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    let status = app.advance_ready(now + Duration::from_millis(80)).unwrap();
    assert!(status.dirty);
    assert_eq!(app.state.count, 1);
}

#[test]
fn app_cx_component_access_preserves_mutation_and_removal_semantics() {
    let now = Instant::now();
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            cx.schedule_after(Duration::ZERO, Action::Mutate);
            cx.schedule_after(Duration::ZERO, Action::Remove);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        |state, action, cx| {
            match action {
                Action::Mutate => {
                    cx.with_component_mut(state.input, |input| input.set_text("changed"));
                    state.count = cx
                        .with_component(state.input, |input| input.text().len())
                        .unwrap();
                }
                Action::Remove => {
                    state.removed = cx.remove_component(state.input).is_some();
                }
                _ => {}
            }
            Ok(())
        },
        |state: &State| View::component(state.input),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    let status = app.advance_ready(now).unwrap();
    assert!(!status.more_ready);
    assert_eq!(app.state.count, 7);
    assert!(app.state.removed);
}

#[test]
fn body_only_apps_have_no_history_and_support_non_send_state() {
    let now = Instant::now();
    let shared = Rc::new(RefCell::new(0));
    let initial = Rc::clone(&shared);
    let app = App::new(
        move |cx: &mut AppCx<'_, Action>| -> Result<Rc<RefCell<usize>>, TestError> {
            assert!(cx.history().is_none());
            assert!(cx.history_mut().is_none());
            cx.schedule_after(Duration::ZERO, Action::Timer);
            Ok(initial)
        },
        move |state, action, cx| {
            if action == Action::Timer {
                *state.borrow_mut() += 1;
                cx.exit();
            }
            Ok(())
        },
        |_state: &Rc<RefCell<usize>>| View::text("body").into_view(),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    let status = app.advance_ready(now).unwrap();
    assert!(status.exiting);
    assert_eq!(*shared.borrow(), 1);
}

#[test]
fn exit_stops_later_queued_actions() {
    let now = Instant::now();
    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            cx.schedule_after(Duration::ZERO, Action::Exit);
            cx.schedule_after(Duration::ZERO, Action::AfterExit);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        |state, action, cx| {
            match action {
                Action::Exit => cx.exit(),
                Action::AfterExit => state.count += 1,
                _ => {}
            }
            Ok(())
        },
        |_state: &State| View::text("body").into_view(),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    let status = app.advance_ready(now).unwrap();
    assert!(status.exiting);
    assert_eq!(app.state.count, 0);
}

#[test]
fn output_route_conflict_remove_and_readd_use_existing_semantics() {
    let now = Instant::now();
    let output = Output::<u32>::new();
    let app = App::new(
        move |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            cx.route(output, |_| Action::Timer)?;
            assert_eq!(cx.route(output, |_| Action::Timer), Err(RouteConflict));
            assert!(cx.remove_route(output));
            assert!(!cx.remove_route(output));
            cx.route(output, |_| Action::Timer)?;
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        |_state, _action, _cx| Ok(()),
        |_state: &State| View::text("body").into_view(),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
}

#[test]
fn application_errors_stop_initialization_or_the_current_batch() {
    let now = Instant::now();
    let init_error = App::new(
        |_cx: &mut AppCx<'_, Action>| -> Result<State, TestError> { Err(TestError::Route) },
        |_state, _action, _cx| Ok(()),
        |_state: &State| View::text("body").into_view(),
    )
    .start(now);
    assert!(matches!(init_error, Err(KernelError::Application(_))));

    let app = App::new(
        |cx: &mut AppCx<'_, Action>| -> Result<State, TestError> {
            cx.schedule_after(Duration::ZERO, Action::A);
            cx.schedule_after(Duration::ZERO, Action::B);
            Ok(State {
                input: cx.register(TextInput::new()),
                submitted: Vec::new(),
                count: 0,
                removed: false,
                ticking: None,
            })
        },
        |_state, action, _cx| {
            if action == Action::A {
                return Err(TestError::Route);
            }
            Ok(())
        },
        |_state: &State| View::text("body").into_view(),
    );
    let mut app = start(app, now);
    prepare(&mut app, now);
    let error = app.advance_ready(now).expect_err("update error");
    assert!(matches!(error, KernelError::Application(_)));
}
