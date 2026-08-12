use std::sync::mpsc::{Receiver, Sender, SyncSender};

use anyhow::{Context, Result, anyhow};
use termwiz::{
    caps::{Capabilities, ProbeHints},
    input::InputEvent,
    surface::{Change, CursorVisibility, Surface},
    terminal::{Terminal, TerminalWaker, new_terminal},
};
use tokio::sync::{oneshot, watch};

type EventSender = tokio::sync::mpsc::UnboundedSender<Result<crate::terminal::TerminalEvent>>;

type Reply<T> = Sender<Result<T>>;
type AsyncReply<T> = oneshot::Sender<Result<T>>;

pub(crate) enum TerminalCommand {
    Present {
        desired: Surface,
        reply: AsyncReply<()>,
    },
    InsertHistory {
        rows: Vec<crate::physical::PhysicalRow>,
        reply: Reply<usize>,
    },
    PositionAfterFinalFrame {
        reply: Reply<()>,
    },
    Restore {
        reply: Reply<()>,
    },
}

pub(crate) struct Startup {
    pub(crate) waker: TerminalWaker,
}

pub(crate) fn run(
    commands: Receiver<TerminalCommand>,
    events: EventSender,
    startup: SyncSender<Result<Startup>>,
    size_sender: watch::Sender<crate::geometry::Size>,
) {
    let setup = setup_terminal();
    let (mut terminal, waker, mut presenter, size) = match setup {
        Ok(value) => value,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };

    size_sender.send_replace(size);
    if startup.send(Ok(Startup { waker })).is_err() {
        presenter.finish_sync_output_best_effort(&mut *terminal);
        let _ = restore_terminal(&mut *terminal);
        return;
    }

    let mut stopping = false;
    while !stopping {
        while let Ok(command) = commands.try_recv() {
            stopping = handle_command(command, &mut *terminal, &mut presenter);
            if stopping {
                break;
            }
        }
        if stopping {
            break;
        }

        match terminal.poll_input(None) {
            Ok(Some(InputEvent::Wake)) => {}
            Ok(Some(InputEvent::Resized { .. })) => {
                let result = terminal
                    .get_screen_size()
                    .and_then(|size| {
                        presenter.finish_sync_output_best_effort(&mut *terminal);
                        presenter.resize(size.cols, size.rows);
                        Ok(crate::geometry::Size::new(
                            u16::try_from(size.cols)
                                .context("terminal width exceeds framework range")?,
                            u16::try_from(size.rows)
                                .context("terminal height exceeds framework range")?,
                        ))
                    })
                    .map_err(anyhow::Error::from);
                match result {
                    Ok(size) => {
                        size_sender.send_replace(size);
                    }
                    Err(error) => {
                        let _ = events.send(Err(error));
                        stopping = true;
                    }
                }
            }
            Ok(Some(event)) => {
                if let Some(event) = super::input::map_input(event)
                    && events.send(Ok(event)).is_err()
                {
                    stopping = true;
                }
            }
            Ok(None) => {}
            Err(error) => {
                let _ = events.send(Err(anyhow!(error)));
                stopping = true;
            }
        }
    }

    presenter.finish_sync_output_best_effort(&mut *terminal);
    let _ = restore_terminal(&mut *terminal);
}

fn setup_terminal() -> Result<(
    Box<dyn Terminal + Send>,
    TerminalWaker,
    super::presenter::TermwizPresenter,
    crate::geometry::Size,
)> {
    let hints = ProbeHints::new_from_env().mouse_reporting(Some(false));
    let capabilities =
        Capabilities::new_with_hints(hints).context("construct terminal capabilities")?;
    let terminal = new_terminal(capabilities).context("open system terminal")?;
    let mut terminal: Box<dyn Terminal + Send> = Box::new(terminal);
    terminal.set_raw_mode().context("set terminal raw mode")?;

    let size = match terminal.get_screen_size().context("get terminal size") {
        Ok(size) => size,
        Err(error) => return setup_error(&mut *terminal, error),
    };
    let hidden = Change::CursorVisibility(CursorVisibility::Hidden);
    let line_breaks = Change::Text("\r\n".repeat(size.rows));
    if let Err(error) = terminal
        .render(&[hidden, line_breaks])
        .and_then(|_| terminal.flush())
        .context("establish main-screen inline viewport")
    {
        return setup_error(&mut *terminal, error);
    }

    let mut presenter = super::presenter::TermwizPresenter::new(size.cols, size.rows);
    if let Err(error) = presenter
        .present(&mut *terminal, Surface::new(size.cols, size.rows))
        .context("paint initial terminal viewport")
    {
        return setup_error(&mut *terminal, error);
    }
    let waker = terminal.waker();
    Ok((
        terminal,
        waker,
        presenter,
        crate::geometry::Size::new(
            u16::try_from(size.cols).context("terminal width exceeds framework range")?,
            u16::try_from(size.rows).context("terminal height exceeds framework range")?,
        ),
    ))
}

fn setup_error<T>(terminal: &mut dyn Terminal, error: anyhow::Error) -> Result<T> {
    let _ = restore_terminal(terminal);
    Err(error)
}

fn handle_command(
    command: TerminalCommand,
    terminal: &mut dyn Terminal,
    presenter: &mut super::presenter::TermwizPresenter,
) -> bool {
    match command {
        TerminalCommand::Present { desired, reply } => {
            let result = presenter.present(terminal, desired);
            let _ = reply.send(result);
            false
        }
        TerminalCommand::InsertHistory { rows, reply } => {
            let result = presenter.insert_history(terminal, &rows);
            let _ = reply.send(result);
            false
        }
        TerminalCommand::PositionAfterFinalFrame { reply } => {
            let result = presenter.position_after_final_frame(terminal);
            let _ = reply.send(result);
            false
        }
        TerminalCommand::Restore { reply } => {
            presenter.finish_sync_output_best_effort(terminal);
            let result = restore_terminal(terminal);
            let _ = reply.send(result);
            true
        }
    }
}

fn restore_terminal(terminal: &mut dyn Terminal) -> Result<()> {
    let mut first_error = None;
    if let Err(error) = terminal
        .render(&[
            Change::AllAttributes(Default::default()),
            Change::CursorVisibility(CursorVisibility::Visible),
        ])
        .and_then(|_| terminal.flush())
    {
        first_error = Some(anyhow!(error));
    }
    if let Err(error) = terminal.set_cooked_mode()
        && first_error.is_none()
    {
        first_error = Some(anyhow!(error));
    }
    if let Err(error) = terminal.flush()
        && first_error.is_none()
    {
        first_error = Some(anyhow!(error));
    }
    first_error.map_or(Ok(()), Err)
}
