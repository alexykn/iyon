use crate::output::{Output, OutputDispatchError, OutputQueue, OutputRouter, RouteConflict};

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Number(usize),
    Text(String),
    Unit,
}

#[test]
fn heterogeneous_events_are_routed_in_strict_fifo_order() {
    let number = Output::<usize>::new();
    let text = Output::<String>::new();
    let unit = Output::<()>::new();
    let unrouted = Output::<bool>::new();
    let mut queue = OutputQueue::new();
    {
        let mut cx = queue.event_cx();
        cx.emit(number, 7);
        cx.emit(text, String::from("text"));
        cx.emit(unrouted, true);
        cx.emit(unit, ());
    }

    let mut router = OutputRouter::<Action>::new();
    router.route(number, Action::Number).unwrap();
    router.route(text, Action::Text).unwrap();
    router.route(unit, |_| Action::Unit).unwrap();

    assert_eq!(
        router.drain(&mut queue).unwrap(),
        vec![
            Action::Number(7),
            Action::Text(String::from("text")),
            Action::Unit
        ]
    );
    assert!(queue.is_empty());
}

#[test]
fn route_conflict_preserves_the_first_route() {
    let output = Output::<usize>::new();
    let mut router = OutputRouter::<usize>::new();
    router.route(output, |value| value + 1).unwrap();

    assert_eq!(
        router.route(output, |value| value + 100),
        Err(RouteConflict)
    );

    let mut queue = OutputQueue::new();
    {
        let mut cx = queue.event_cx();
        cx.emit(output, 1);
    }
    assert_eq!(router.drain(&mut queue).unwrap(), vec![2]);
}

#[test]
fn removal_disables_a_route_and_allows_re_registration() {
    let output = Output::<usize>::new();
    let mut router = OutputRouter::<usize>::new();
    router.route(output, |value| value + 1).unwrap();

    assert!(router.remove(output));
    assert!(!router.remove(output));

    let mut queue = OutputQueue::new();
    {
        let mut cx = queue.event_cx();
        cx.emit(output, 1);
    }
    assert!(router.drain(&mut queue).unwrap().is_empty());

    router.route(output, |value| value + 10).unwrap();
    {
        let mut cx = queue.event_cx();
        cx.emit(output, 1);
    }
    assert_eq!(router.drain(&mut queue).unwrap(), vec![11]);
}

#[test]
fn fresh_outputs_never_match_stale_routes() {
    let old = Output::<usize>::new();
    let fresh = Output::<usize>::new();
    let mut router = OutputRouter::<usize>::new();
    router.route(old, |value| value).unwrap();

    let mut queue = OutputQueue::new();
    {
        let mut cx = queue.event_cx();
        cx.emit(fresh, 9);
    }
    assert!(router.drain(&mut queue).unwrap().is_empty());
}

#[test]
fn forged_internal_type_mismatch_fails_loudly() {
    let output = Output::<usize>::new();
    let mut router = OutputRouter::<usize>::new();
    router.route(output, |value| value).unwrap();

    let mut queue = OutputQueue::new();
    queue.push_mismatched_for_test(output, String::from("wrong"));

    assert_eq!(
        router.drain(&mut queue),
        Err(OutputDispatchError::TypeMismatch)
    );
}

#[test]
fn several_dispatch_boundaries_preserve_their_sequential_order() {
    let output = Output::<usize>::new();
    let mut router = OutputRouter::<usize>::new();
    router.route(output, |value| value).unwrap();
    let mut queue = OutputQueue::new();

    {
        let mut cx = queue.event_cx();
        cx.emit(output, 1);
        cx.emit(output, 2);
        cx.emit(output, 3);
    }
    assert_eq!(router.drain(&mut queue).unwrap(), vec![1, 2, 3]);

    {
        let mut cx = queue.event_cx();
        cx.emit(output, 4);
        cx.emit(output, 5);
    }
    assert_eq!(router.drain(&mut queue).unwrap(), vec![4, 5]);
}
