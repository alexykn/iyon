use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use super::{Component, ComponentHandle, ComponentId, MountGraph, MountTransitions};
use crate::component::ComponentRegistry;

trait TickDriver {
    fn tick(&mut self, handle: ComponentId, now: Instant, registry: &mut ComponentRegistry)
    -> bool;
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
    ) -> bool {
        registry
            .with_mut(ComponentHandle::<C>::from_id(handle), |component| {
                (self.callback)(component, now)
            })
            .unwrap_or(false)
    }
}

struct TickRegistration {
    interval: Duration,
    next_due: Option<Instant>,
    driver: Box<dyn TickDriver>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TickRegistrationError {
    ZeroInterval,
    AlreadyRegistered,
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
            },
        );
        Ok(())
    }

    /// Synchronizes activation from the semantic mount graph. Transitions are
    /// consumed for deterministic activation/deactivation, while the graph is
    /// authoritative for registrations created between reconciliations.
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

    pub(crate) fn next_timeout(&self, now: Instant, idle_timeout: Duration) -> Duration {
        self.mount_order
            .iter()
            .filter_map(|id| self.registrations.get(id))
            .filter_map(|registration| registration.next_due)
            .map(|deadline| deadline.checked_duration_since(now).unwrap_or_default())
            .min()
            .unwrap_or(idle_timeout)
    }

    pub(crate) fn tick_due(&mut self, now: Instant, registry: &mut ComponentRegistry) -> bool {
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
        for id in due {
            let Some(registration) = self.registrations.get_mut(&id) else {
                continue;
            };
            dirty |= registration.driver.tick(id, now, registry);
            registration.next_due = Some(
                now.checked_add(registration.interval)
                    .expect("component tick deadline exhausted"),
            );
        }
        dirty
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
