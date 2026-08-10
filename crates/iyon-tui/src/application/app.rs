use crate::{History, View};

use super::{context::AppCx, kernel::RunningApp};

/// A generic standalone application definition.
///
/// `App` stores application State, Action, and the three semantic callbacks.
/// It has no terminal or executor dependency. The production terminal driver
/// is added in S10.
pub struct App<State, Action, Error, Init, Update, ViewFn> {
    pub(crate) init: Init,
    pub(crate) update: Update,
    pub(crate) view: ViewFn,
    pub(crate) history: Option<History>,
    pub(crate) marker: std::marker::PhantomData<fn(State, Action) -> Error>,
}

impl<State, Action, Error, Init, Update, ViewFn> App<State, Action, Error, Init, Update, ViewFn> {
    /// Defines an application from initialization, action update, and body
    /// derivation callbacks.
    pub fn new(init: Init, update: Update, view: ViewFn) -> Self
    where
        Init: FnOnce(&mut AppCx<'_, Action>) -> Result<State, Error>,
        Update: FnMut(&mut State, Action, &mut AppCx<'_, Action>) -> Result<(), Error>,
        ViewFn: Fn(&State) -> View,
    {
        Self {
            init,
            update,
            view,
            history: None,
            marker: std::marker::PhantomData,
        }
    }

    /// Configures the one persistent root History owned by this application.
    pub fn with_history(mut self, history: History) -> Self {
        self.history = Some(history);
        self
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "S9 private startup seam is consumed by the runtime driver"
        )
    )]
    pub(crate) fn start(
        self,
        now: std::time::Instant,
    ) -> Result<RunningApp<State, Action, Error, Update, ViewFn>, super::kernel::KernelError<Error>>
    where
        Init: FnOnce(&mut AppCx<'_, Action>) -> Result<State, Error>,
        Update: FnMut(&mut State, Action, &mut AppCx<'_, Action>) -> Result<(), Error>,
        ViewFn: Fn(&State) -> View,
    {
        RunningApp::new(self, now)
    }
}
