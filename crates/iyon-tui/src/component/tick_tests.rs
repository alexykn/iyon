use std::time::{Duration, Instant};

use super::*;
use crate::presentation::{IntoView, View};

#[derive(Debug)]
struct Blinker {
    frame: u8,
}

impl Component for Blinker {
    fn view(&self) -> View {
        View::text(self.frame.to_string()).into_view()
    }
}

fn graph(ids: &[(ComponentId, Option<ComponentId>)]) -> MountGraph {
    MountGraph::new(
        ids.iter()
            .map(|(id, parent)| MountNode {
                id: *id,
                parent: *parent,
                revision: ComponentRevision::default(),
            })
            .collect(),
    )
}

#[test]
fn one_mounted_component_ticks_only_after_its_deadline() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Blinker { frame: 0 });
    let mut scheduler = TickScheduler::new();
    scheduler
        .register(handle, Duration::from_millis(80), |blinker, _| {
            blinker.frame += 1;
            true
        })
        .unwrap();

    let start = Instant::now();
    let mut mounted = MountedComponents::default();
    let transitions = mounted.reconcile(graph(&[(handle.id(), None)]));
    scheduler.sync_mounts(mounted.current(), &transitions, start);
    assert_eq!(
        scheduler.next_timeout(start, Duration::from_secs(1)),
        Duration::from_millis(80)
    );
    assert!(!scheduler.tick_due(start + Duration::from_millis(79), &mut registry));
    assert_eq!(registry.with(handle, |blinker| blinker.frame), Some(0));
    assert!(scheduler.tick_due(start + Duration::from_millis(80), &mut registry));
    assert_eq!(registry.with(handle, |blinker| blinker.frame), Some(1));
    assert_eq!(registry.revision(handle).unwrap().value(), 1);
}

#[test]
fn multiple_due_components_tick_in_mount_order() {
    let mut registry = ComponentRegistry::new();
    let first = registry.register(Blinker { frame: 0 });
    let second = registry.register(Blinker { frame: 0 });
    let mut scheduler = TickScheduler::new();
    scheduler
        .register(first, Duration::from_millis(80), |blinker, _| {
            blinker.frame += 1;
            true
        })
        .unwrap();
    scheduler
        .register(second, Duration::from_millis(80), |blinker, _| {
            blinker.frame += 1;
            true
        })
        .unwrap();

    let now = Instant::now();
    let mut mounted = MountedComponents::default();
    let transitions = mounted.reconcile(graph(&[(first.id(), None), (second.id(), None)]));
    scheduler.sync_mounts(mounted.current(), &transitions, now);
    assert!(scheduler.tick_due(now + Duration::from_millis(80), &mut registry));
    assert_eq!(registry.with(first, |blinker| blinker.frame), Some(1));
    assert_eq!(registry.with(second, |blinker| blinker.frame), Some(1));
    assert_eq!(registry.revision(first).unwrap().value(), 1);
    assert_eq!(registry.revision(second).unwrap().value(), 1);
}

#[test]
fn unmount_deactivates_and_remount_resets_the_deadline() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Blinker { frame: 0 });
    let mut scheduler = TickScheduler::new();
    scheduler
        .register(handle, Duration::from_millis(80), |blinker, _| {
            blinker.frame += 1;
            true
        })
        .unwrap();
    let initial = Instant::now();
    let mut mounted = MountedComponents::default();
    let transitions = mounted.reconcile(graph(&[(handle.id(), None)]));
    scheduler.sync_mounts(mounted.current(), &transitions, initial);

    let transitions = mounted.reconcile(MountGraph::default());
    scheduler.sync_mounts(
        mounted.current(),
        &transitions,
        initial + Duration::from_millis(100),
    );
    assert!(!scheduler.tick_due(initial + Duration::from_secs(1), &mut registry));
    assert_eq!(registry.with(handle, |blinker| blinker.frame), Some(0));

    let remount = initial + Duration::from_secs(2);
    let transitions = mounted.reconcile(graph(&[(handle.id(), None)]));
    scheduler.sync_mounts(mounted.current(), &transitions, remount);
    assert!(!scheduler.tick_due(remount, &mut registry));
    assert!(scheduler.tick_due(remount + Duration::from_millis(80), &mut registry));
    assert_eq!(registry.with(handle, |blinker| blinker.frame), Some(1));
}

#[test]
fn different_intervals_select_the_earliest_deadline_independently() {
    let mut registry = ComponentRegistry::new();
    let fast = registry.register(Blinker { frame: 0 });
    let slow = registry.register(Blinker { frame: 0 });
    let mut scheduler = TickScheduler::new();
    scheduler
        .register(fast, Duration::from_millis(80), |blinker, _| {
            blinker.frame += 1;
            true
        })
        .unwrap();
    scheduler
        .register(slow, Duration::from_millis(200), |blinker, _| {
            blinker.frame += 1;
            true
        })
        .unwrap();

    let start = Instant::now();
    let mut mounted = MountedComponents::default();
    let transitions = mounted.reconcile(graph(&[(fast.id(), None), (slow.id(), None)]));
    scheduler.sync_mounts(mounted.current(), &transitions, start);
    assert_eq!(
        scheduler.next_timeout(start, Duration::from_secs(1)),
        Duration::from_millis(80)
    );
    scheduler.tick_due(start + Duration::from_millis(80), &mut registry);
    assert_eq!(registry.with(fast, |blinker| blinker.frame), Some(1));
    assert_eq!(registry.with(slow, |blinker| blinker.frame), Some(0));
    assert_eq!(
        scheduler.next_timeout(start + Duration::from_millis(80), Duration::from_secs(1)),
        Duration::from_millis(80)
    );
    scheduler.tick_due(start + Duration::from_millis(200), &mut registry);
    assert_eq!(registry.with(slow, |blinker| blinker.frame), Some(1));
}

#[test]
fn zero_intervals_and_duplicate_registrations_are_rejected() {
    let mut registry = ComponentRegistry::new();
    let handle = registry.register(Blinker { frame: 0 });
    let mut scheduler = TickScheduler::new();
    assert_eq!(
        scheduler.register(handle, Duration::ZERO, |_, _| true),
        Err(TickRegistrationError::ZeroInterval)
    );
    scheduler
        .register(handle, Duration::from_millis(1), |_, _| true)
        .unwrap();
    assert_eq!(
        scheduler.register(handle, Duration::from_millis(1), |_, _| true),
        Err(TickRegistrationError::AlreadyRegistered)
    );
}
