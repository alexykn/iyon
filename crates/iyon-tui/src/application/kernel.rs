use std::{collections::VecDeque, marker::PhantomData, time::Instant};

use anyhow::Result;

use crate::{
    InteractionResult, OutputRouter, Scene, View,
    backend::NativeHistorySink,
    component::ComponentRegistry,
    geometry::Size,
    output::OutputDispatchError,
    scene::{PreparedSceneFrame, SceneHost, SceneHostError},
};

use super::{app::App, context::AppCx, timer::TimerQueue};

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 kernel batching is consumed by the future runtime driver"
    )
)]
const ACTION_BATCH_BUDGET: usize = 128;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 kernel status errors are consumed by the future runtime driver"
    )
)]
#[derive(Debug)]
pub(crate) enum KernelError<Error> {
    Application(Error),
    Output(OutputDispatchError),
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 ready status is consumed by the future runtime driver"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadyStatus {
    pub(crate) dirty: bool,
    pub(crate) exiting: bool,
    pub(crate) more_ready: bool,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 running kernel is consumed by the headless proof driver and S10 runtime"
    )
)]
pub(crate) struct RunningApp<State, Action, Error, Update, ViewFn> {
    pub(crate) state: State,
    scene: Scene,
    components: ComponentRegistry,
    outputs: OutputRouter<Action>,
    scene_host: SceneHost,
    actions: VecDeque<Action>,
    timers: TimerQueue<Action>,
    update: Update,
    view: ViewFn,
    dirty: bool,
    body_dirty: bool,
    exit_requested: bool,
    marker: PhantomData<fn() -> Error>,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S9 kernel operations are consumed by the headless proof driver and S10 runtime"
    )
)]
impl<State, Action, Error, Update, ViewFn> RunningApp<State, Action, Error, Update, ViewFn>
where
    Update: FnMut(&mut State, Action, &mut AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> View,
{
    pub(crate) fn new<Init>(
        app: App<State, Action, Error, Init, Update, ViewFn>,
        now: Instant,
    ) -> Result<Self, KernelError<Error>>
    where
        Init: FnOnce(&mut AppCx<'_, Action>) -> Result<State, Error>,
    {
        let App {
            init,
            update,
            view,
            history,
            marker: _,
        } = app;
        let mut scene = history.map_or_else(
            || Scene::new(View::spacer(0)),
            |history| Scene::with_history(history, View::spacer(0)),
        );
        let mut components = ComponentRegistry::new();
        let mut outputs = OutputRouter::new();
        let mut timers = TimerQueue::default();
        let mut exit_requested = false;
        let state = {
            let mut cx = AppCx::new(
                &mut scene,
                &mut components,
                &mut outputs,
                &mut timers,
                &mut exit_requested,
                now,
            );
            init(&mut cx).map_err(KernelError::Application)?
        };
        let mut running = Self {
            state,
            scene,
            components,
            outputs,
            scene_host: SceneHost::default(),
            actions: VecDeque::new(),
            timers,
            update,
            view,
            dirty: true,
            body_dirty: false,
            exit_requested,
            marker: PhantomData,
        };
        let body = (running.view)(&running.state);
        running.scene.set_body(body);
        Ok(running)
    }

    pub(crate) fn dispatch_key(
        &mut self,
        key: crate::KeyStroke,
    ) -> Result<InteractionResult, KernelError<Error>> {
        if self.exit_requested {
            return Ok(InteractionResult::Ignored);
        }
        let result = self.scene_host.dispatch_key(key, &mut self.components);
        self.drain_outputs_to_actions()?;
        if result == InteractionResult::Consumed {
            self.dirty = true;
        }
        Ok(result)
    }

    pub(crate) fn dispatch_paste(
        &mut self,
        text: &str,
    ) -> Result<InteractionResult, KernelError<Error>> {
        if self.exit_requested {
            return Ok(InteractionResult::Ignored);
        }
        let result = self.scene_host.dispatch_paste(text, &mut self.components);
        self.drain_outputs_to_actions()?;
        if result == InteractionResult::Consumed {
            self.dirty = true;
        }
        Ok(result)
    }

    pub(crate) fn advance_ready(
        &mut self,
        now: Instant,
    ) -> Result<ReadyStatus, KernelError<Error>> {
        if self.exit_requested {
            self.actions.clear();
            self.timers.clear();
            return Ok(self.status(false));
        }

        self.collect_due_timers(now);
        let tick = self.scene_host.tick_due(now, &mut self.components);
        self.dirty |= tick.dirty;
        self.drain_outputs_to_actions()?;

        for _ in 0..ACTION_BATCH_BUDGET {
            let Some(action) = self.actions.pop_front() else {
                break;
            };
            let update_result = {
                let mut cx = AppCx::new(
                    &mut self.scene,
                    &mut self.components,
                    &mut self.outputs,
                    &mut self.timers,
                    &mut self.exit_requested,
                    now,
                );
                (self.update)(&mut self.state, action, &mut cx)
            };
            update_result.map_err(KernelError::Application)?;
            self.dirty = true;
            self.body_dirty = true;
            self.drain_outputs_to_actions()?;
            self.collect_due_timers(now);
            if self.exit_requested {
                self.actions.clear();
                self.timers.clear();
                break;
            }
        }

        Ok(self.status(!self.actions.is_empty()))
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        [
            self.timers.next_deadline(),
            self.scene_host.next_tick_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(crate) fn prepare_frame<S, F>(
        &mut self,
        now: Instant,
        sink: &mut S,
        mut viewport: F,
    ) -> Result<PreparedSceneFrame, SceneHostError<S::Error>>
    where
        S: NativeHistorySink,
        F: FnMut(&mut S) -> Result<Size>,
    {
        if self.body_dirty {
            let body = (self.view)(&self.state);
            self.scene.set_body(body);
            self.body_dirty = false;
        }
        let frame = self.scene_host.render_at(
            now,
            &mut self.scene,
            &mut self.components,
            sink,
            &mut viewport,
        )?;
        self.dirty = false;
        Ok(frame)
    }

    #[cfg(test)]
    pub(crate) fn focused_for_test(&self) -> bool {
        self.scene_host.focused().is_some()
    }

    #[cfg(test)]
    pub(crate) fn mount_count_for_test(&self) -> usize {
        self.scene_host.mount_count_for_test()
    }

    #[cfg(test)]
    pub(crate) fn focusable_count_for_test(&self) -> usize {
        self.scene_host.focusable_count_for_test()
    }

    fn status(&self, more_ready: bool) -> ReadyStatus {
        ReadyStatus {
            dirty: self.dirty,
            exiting: self.exit_requested,
            more_ready: more_ready && !self.exit_requested,
        }
    }

    fn collect_due_timers(&mut self, now: Instant) {
        while let Some(action) = self.timers.pop_due(now) {
            self.actions.push_back(action);
        }
    }

    fn drain_outputs_to_actions(&mut self) -> Result<(), KernelError<Error>> {
        let actions = self
            .scene_host
            .drain_outputs(&self.outputs)
            .map_err(KernelError::Output)?;
        self.actions.extend(actions);
        Ok(())
    }
}

impl<Error> From<OutputDispatchError> for KernelError<Error> {
    fn from(error: OutputDispatchError) -> Self {
        Self::Output(error)
    }
}
