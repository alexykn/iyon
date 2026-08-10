use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use super::{Component, ComponentHandle, ComponentId, MountGraph, MountTransitions};
use crate::{
    component::ComponentRegistry,
    interaction::MountedCapabilities,
    output::{EventCx, OutputQueue},
};

trait TickDriver {
    fn tick(
        &mut self,
        handle: ComponentId,
        now: Instant,
        registry: &mut ComponentRegistry,
        cx: &mut EventCx<'_>,
    ) -> bool;
}

struct TypedTickDriver<C, F> {
    callback: F,
    marker: std::marker::PhantomData<fn() -> C>,
}

impl<C, F> TickDriver for TypedTickDriver<C, F>
where
    C: Component,
    F: FnMut(&mut C, Instant) -> bool,
{
    fn tick(
        &mut self,
        handle: ComponentId,
        now: Instant,
        registry: &mut ComponentRegistry,
        _cx: &mut EventCx<'_>,
    ) -> bool {
        registry
            .with_mut(ComponentHandle::<C>::from_id(handle), |component| {
                (self.callback)(component, now)
            })
            .unwrap_or(false)
    }
}

struct CapabilityTickDriver {
    callback: Arc<dyn for<'a> Fn(&mut dyn std::any::Any, Instant, &mut EventCx<'a>) -> bool>,
}

impl TickDriver for CapabilityTickDriver {
    fn tick(
        &mut self,
        handle: ComponentId,
        now: Instant,
        registry: &mut ComponentRegistry,
        cx: &mut EventCx<'_>,
    ) -> bool {
        registry
            .with_any_mut(handle, |component| (self.callback)(component, now, cx))
            .unwrap_or(false)
    }
}

struct TickRegistration {
    interval: Duration,
    next_due: Option<Instant>,
    driver: Box<dyn TickDriver>,
    source: TickSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TickSource {
    Legacy,
    Capability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TickRegistrationError {
    ZeroInterval,
    AlreadyRegistered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TickOutcome {
    pub(crate) ran: bool,
    pub(crate) dirty: bool,
}

/// Private scheduler for mounted retained components.
pub(crate) struct TickScheduler {
    registrations: HashMap<ComponentId, TickRegistration>,
    mounted: HashSet<ComponentId>,
    mount_order: Vec<ComponentId>,
}

impl Default for TickScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl TickScheduler {
    pub(crate) fn new() -> Self {
        Self {
            registrations: HashMap::new(),
            mounted: HashSet::new(),
            mount_order: Vec::new(),
        }
    }

    pub(crate) fn register<C, F>(
        &mut self,
        handle: ComponentHandle<C>,
        interval: Duration,
        callback: F,
    ) -> Result<(), TickRegistrationError>
    where
        C: Component,
        F: FnMut(&mut C, Instant) -> bool + 'static,
    {
        if interval.is_zero() {
            return Err(TickRegistrationError::ZeroInterval);
        }
        if self.registrations.contains_key(&handle.id()) {
            return Err(TickRegistrationError::AlreadyRegistered);
        }

        self.registrations.insert(
            handle.id(),
            TickRegistration {
                interval,
                next_due: None,
                driver: Box::new(TypedTickDriver {
                    callback,
                    marker: std::marker::PhantomData,
                }),
                source: TickSource::Legacy,
            },
        );
        Ok(())
    }

    /// Synchronizes activation from the semantic mount graph.
    pub(crate) fn sync_mounts(
        &mut self,
        graph: &MountGraph,
        transitions: &MountTransitions,
        now: Instant,
    ) {
        for transition in &transitions.transitions {
            match transition {
                super::MountTransition::Mounted { id, .. } => {
                    self.mounted.insert(*id);
                    self.activate(*id, now);
                }
                super::MountTransition::Unmounted { id } => {
                    self.mounted.remove(id);
                    self.deactivate(*id);
                }
            }
        }

        let graph_ids: HashSet<_> = graph.ids().collect();
        for id in &graph_ids {
            if self.mounted.insert(*id) {
                self.activate(*id, now);
            } else if self
                .registrations
                .get(id)
                .is_some_and(|registration| registration.next_due.is_none())
            {
                self.activate(*id, now);
            }
        }
        for id in self.mounted.clone() {
            if !graph_ids.contains(&id) {
                self.mounted.remove(&id);
                self.deactivate(id);
            }
        }
        self.mount_order = graph.ids().collect();
    }

    /// Synchronizes the scheduler's current typed tick declarations with a
    /// successfully resolved mounted capability set.
    pub(crate) fn sync_capabilities(
        &mut self,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
        transitions: &MountTransitions,
        now: Instant,
    ) {
        self.sync_mounts(graph, transitions, now);

        let desired: HashMap<_, _> = graph
            .ids()
            .filter_map(|id| {
                capabilities
                    .get(id)
                    .and_then(|caps| caps.tick.clone().map(|tick| (id, tick)))
            })
            .collect();

        for (id, tick) in &desired {
            if tick.interval.is_zero() {
                continue;
            }
            let driver = Box::new(CapabilityTickDriver {
                callback: tick.handler.clone(),
            });
            match self.registrations.get_mut(id) {
                Some(registration) if registration.source == TickSource::Capability => {
                    let interval_changed = registration.interval != tick.interval;
                    registration.interval = tick.interval;
                    registration.driver = driver;
                    if interval_changed {
                        registration.next_due = None;
                        self.activate(*id, now);
                    } else if registration.next_due.is_none() {
                        self.activate(*id, now);
                    }
                }
                Some(registration) => {
                    registration.interval = tick.interval;
                    registration.next_due = None;
                    registration.driver = driver;
                    registration.source = TickSource::Capability;
                    self.activate(*id, now);
                }
                None => {
                    self.registrations.insert(
                        *id,
                        TickRegistration {
                            interval: tick.interval,
                            next_due: None,
                            driver,
                            source: TickSource::Capability,
                        },
                    );
                    self.activate(*id, now);
                }
            }
        }

        let stale: Vec<_> = self
            .registrations
            .iter()
            .filter(|(id, registration)| {
                registration.source == TickSource::Capability && !desired.contains_key(id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            self.registrations.remove(&id);
        }
    }

    pub(crate) fn next_timeout(&self, now: Instant, idle_timeout: Duration) -> Duration {
        self.mount_order
            .iter()
            .filter_map(|id| self.registrations.get(id))
            .filter_map(|registration| registration.next_due)
            .map(|deadline| deadline.checked_duration_since(now).unwrap_or_default())
            .min()
            .unwrap_or(idle_timeout)
    }

    pub(crate) fn tick_due(
        &mut self,
        now: Instant,
        registry: &mut ComponentRegistry,
    ) -> TickOutcome {
        let mut queue = OutputQueue::new();
        self.tick_due_with_events(now, registry, &mut queue)
    }

    pub(crate) fn tick_due_with_events(
        &mut self,
        now: Instant,
        registry: &mut ComponentRegistry,
        queue: &mut OutputQueue,
    ) -> TickOutcome {
        let due: Vec<_> = self
            .mount_order
            .iter()
            .copied()
            .filter(|id| {
                self.registrations
                    .get(id)
                    .and_then(|registration| registration.next_due)
                    .is_some_and(|deadline| deadline <= now)
            })
            .collect();

        let mut dirty = false;
        let mut ran = false;
        let mut cx = queue.event_cx();
        for id in due {
            let Some(registration) = self.registrations.get_mut(&id) else {
                continue;
            };
            ran = true;
            dirty |= registration.driver.tick(id, now, registry, &mut cx);
            registration.next_due = Some(
                now.checked_add(registration.interval)
                    .expect("component tick deadline exhausted"),
            );
        }
        TickOutcome { ran, dirty }
    }

    fn activate(&mut self, id: ComponentId, now: Instant) {
        if let Some(registration) = self.registrations.get_mut(&id) {
            registration.next_due = Some(
                now.checked_add(registration.interval)
                    .expect("component tick deadline exhausted"),
            );
        }
    }

    fn deactivate(&mut self, id: ComponentId) {
        if let Some(registration) = self.registrations.get_mut(&id) {
            registration.next_due = None;
        }
    }
}
