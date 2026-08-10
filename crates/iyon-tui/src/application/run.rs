use std::{future::pending, time::Instant};

use crate::terminal::{TerminalBackend, TerminalEvent};

use super::{
    app::App,
    error::{RunError, RuntimeError},
    kernel::{KernelError, RunningApp},
};

pub(crate) async fn run<State, Action, Error, Init, Update, ViewFn>(
    app: App<State, Action, Error, Init, Update, ViewFn>,
) -> Result<(), RunError<Error>>
where
    Init: FnOnce(&mut super::context::AppCx<'_, Action>) -> Result<State, Error>,
    Update: FnMut(&mut State, Action, &mut super::context::AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> crate::View,
{
    let now = Instant::now();
    let mut app = app.start(now).map_err(map_kernel_error)?;
    if app.is_exiting() {
        app.close_ingress();
        return Ok(());
    }
    let backend = crate::terminal::enter_default().map_err(runtime_error)?;
    run_running(app, backend).await
}

#[cfg(test)]
pub(crate) async fn run_with_backend<State, Action, Error, Init, Update, ViewFn, Backend>(
    app: App<State, Action, Error, Init, Update, ViewFn>,
    backend: Backend,
) -> Result<(), RunError<Error>>
where
    Init: FnOnce(&mut super::context::AppCx<'_, Action>) -> Result<State, Error>,
    Update: FnMut(&mut State, Action, &mut super::context::AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> crate::View,
    Backend: TerminalBackend,
{
    run_with_backend_started(app, backend).await
}

#[cfg(test)]
async fn run_with_backend_started<State, Action, Error, Init, Update, ViewFn, Backend>(
    app: App<State, Action, Error, Init, Update, ViewFn>,
    backend: Backend,
) -> Result<(), RunError<Error>>
where
    Init: FnOnce(&mut super::context::AppCx<'_, Action>) -> Result<State, Error>,
    Update: FnMut(&mut State, Action, &mut super::context::AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> crate::View,
    Backend: TerminalBackend,
{
    let now = Instant::now();
    let mut app = app.start(now).map_err(map_kernel_error)?;
    if app.is_exiting() {
        app.close_ingress();
        return Ok(());
    }
    run_running(app, backend).await
}

async fn run_running<State, Action, Error, Update, ViewFn, Backend>(
    mut app: RunningApp<State, Action, Error, Update, ViewFn>,
    backend: Backend,
) -> Result<(), RunError<Error>>
where
    Update: FnMut(&mut State, Action, &mut super::context::AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> crate::View,
    Backend: TerminalBackend,
{
    let mut session = TerminalSession::new(backend);
    let result = drive(&mut app, &mut session).await;
    match result {
        Ok(()) => {
            session
                .position_after_final_frame()
                .map_err(|error| RunError::Runtime(runtime_error(error)))?;
            session
                .restore()
                .map_err(|error| RunError::Runtime(runtime_error(error)))
        }
        Err(error) => {
            let _ = session.restore();
            Err(error)
        }
    }
}

async fn drive<State, Action, Error, Update, ViewFn, Backend>(
    app: &mut RunningApp<State, Action, Error, Update, ViewFn>,
    session: &mut TerminalSession<Backend>,
) -> Result<(), RunError<Error>>
where
    Update: FnMut(&mut State, Action, &mut super::context::AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> crate::View,
    Backend: TerminalBackend,
{
    prepare_and_draw(app, session, Instant::now()).await?;
    app.collect_external_pending();

    loop {
        let now = Instant::now();
        let status = app.advance_ready(now).map_err(map_kernel_error)?;
        if status.dirty {
            prepare_and_draw(app, session, now).await?;
        }
        if status.exiting {
            break;
        }
        if status.more_ready {
            tokio::task::yield_now().await;
            continue;
        }

        let deadline = app.next_deadline();
        tokio::select! {
            event = session.next_event() => {
                match event
                    .map_err(|error| RunError::Runtime(runtime_error(error)))?
                {
                    TerminalEvent::Key(key) => {
                        app.dispatch_key(key).map_err(map_kernel_error)?;
                    }
                    TerminalEvent::Paste(text) => {
                        app.dispatch_paste(&text).map_err(map_kernel_error)?;
                    }
                    TerminalEvent::Resize => app.invalidate_frame(),
                }
            }
            action = app.recv_external(), if app.ingress_is_open() => {
                match action {
                    Some(action) => app.collect_external(action),
                    None => app.close_ingress(),
                }
            }
            _ = wait_for_deadline(deadline) => {}
        }
    }

    Ok(())
}

async fn prepare_and_draw<State, Action, Error, Update, ViewFn, Backend>(
    app: &mut RunningApp<State, Action, Error, Update, ViewFn>,
    session: &mut TerminalSession<Backend>,
    now: Instant,
) -> Result<(), RunError<Error>>
where
    Update: FnMut(&mut State, Action, &mut super::context::AppCx<'_, Action>) -> Result<(), Error>,
    ViewFn: Fn(&State) -> crate::View,
    Backend: TerminalBackend,
{
    let frame = app
        .prepare_frame(now, session.backend_mut(), |backend| backend.viewport())
        .map_err(|error| RunError::Runtime(RuntimeError::message(format!("{error:?}"))))?;
    session
        .draw_frame(&frame)
        .map_err(|error| RunError::Runtime(runtime_error(error)))
}

async fn wait_for_deadline(deadline: Option<Instant>) {
    let Some(deadline) = deadline else {
        pending::<()>().await;
        return;
    };
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
}

struct TerminalSession<Backend: TerminalBackend> {
    backend: Backend,
    restored: bool,
}

impl<Backend> TerminalSession<Backend>
where
    Backend: TerminalBackend,
{
    fn new(backend: Backend) -> Self {
        Self {
            backend,
            restored: false,
        }
    }

    fn backend_mut(&mut self) -> &mut Backend {
        &mut self.backend
    }

    fn next_event(
        &mut self,
    ) -> impl std::future::Future<Output = anyhow::Result<TerminalEvent>> + '_ {
        self.backend.next_event()
    }

    fn draw_frame(&mut self, frame: &crate::scene::PreparedSceneFrame) -> anyhow::Result<()> {
        self.backend.draw_frame(frame)
    }

    fn position_after_final_frame(&mut self) -> anyhow::Result<()> {
        self.backend.position_after_final_frame()
    }

    fn restore(&mut self) -> anyhow::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.restored = true;
        self.backend.restore()
    }
}

impl<Backend> Drop for TerminalSession<Backend>
where
    Backend: TerminalBackend,
{
    fn drop(&mut self) {
        if !self.restored {
            self.restored = true;
            let _ = self.backend.restore();
        }
    }
}

fn runtime_error(error: impl Into<anyhow::Error>) -> RuntimeError {
    RuntimeError::new(error)
}

fn map_kernel_error<Error>(error: KernelError<Error>) -> RunError<Error> {
    match error {
        KernelError::Application(error) => RunError::Application(error),
        KernelError::Output(error) => RunError::Runtime(RuntimeError::message(error.to_string())),
    }
}
