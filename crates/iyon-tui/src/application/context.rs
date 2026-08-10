use std::time::{Duration, Instant};

use crate::{
    Component, ComponentHandle, History, Output, OutputRouter, RouteConflict,
    component::ComponentRegistry, scene::Scene,
};

use super::timer::{TimerHandle, TimerQueue};

/// Borrow-scoped application capabilities available during initialization and
/// action updates.
pub struct AppCx<'a, Action> {
    scene: &'a mut Scene,
    components: &'a mut ComponentRegistry,
    outputs: &'a mut OutputRouter<Action>,
    timers: &'a mut TimerQueue<Action>,
    exit_requested: &'a mut bool,
    now: Instant,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 capability facade is consumed by application startup and action updates"
    )
)]
impl<'a, Action> AppCx<'a, Action> {
    pub(crate) fn new(
        scene: &'a mut Scene,
        components: &'a mut ComponentRegistry,
        outputs: &'a mut OutputRouter<Action>,
        timers: &'a mut TimerQueue<Action>,
        exit_requested: &'a mut bool,
        now: Instant,
    ) -> Self {
        Self {
            scene,
            components,
            outputs,
            timers,
            exit_requested,
            now,
        }
    }

    /// Registers one retained component and returns its typed identity.
    pub fn register<C>(&mut self, component: C) -> ComponentHandle<C>
    where
        C: Component,
    {
        self.components.register(component)
    }

    /// Provides immutable closure-scoped access to a registered component.
    pub fn with_component<C, R>(
        &self,
        handle: ComponentHandle<C>,
        access: impl FnOnce(&C) -> R,
    ) -> Option<R>
    where
        C: Component,
    {
        self.components.with(handle, access)
    }

    /// Provides mutable closure-scoped access to a registered component.
    pub fn with_component_mut<C, R>(
        &mut self,
        handle: ComponentHandle<C>,
        access: impl FnOnce(&mut C) -> R,
    ) -> Option<R>
    where
        C: Component,
    {
        self.components.with_mut(handle, access)
    }

    /// Removes a registered component after application code has released all
    /// semantic references to it.
    pub fn remove_component<C>(&mut self, handle: ComponentHandle<C>) -> Option<C>
    where
        C: Component,
    {
        self.components.remove(handle)
    }

    /// Routes one typed component output into an application action.
    pub fn route<T: 'static>(
        &mut self,
        output: Output<T>,
        map: impl Fn(T) -> Action + 'static,
    ) -> Result<(), RouteConflict> {
        self.outputs.route(output, map)
    }

    /// Removes an application route for one typed component output.
    pub fn remove_route<T: 'static>(&mut self, output: Output<T>) -> bool {
        self.outputs.remove(output)
    }

    /// Returns the persistent root History, when this App was configured with
    /// one.
    pub fn history(&self) -> Option<&History> {
        self.scene.history()
    }

    /// Returns mutable access to the persistent root History, when configured.
    pub fn history_mut(&mut self) -> Option<&mut History> {
        self.scene.history_mut()
    }

    /// Schedules one non-recurring Action.
    pub fn schedule_after(&mut self, delay: Duration, action: Action) -> TimerHandle {
        self.timers.schedule(self.now, delay, action)
    }

    /// Cancels a pending timer. A fired or already-cancelled timer returns
    /// `false`.
    pub fn cancel_timer(&mut self, handle: TimerHandle) -> bool {
        self.timers.cancel(handle)
    }

    /// Requests semantic application exit after the current update returns.
    pub fn exit(&mut self) {
        *self.exit_requested = true;
    }
}
