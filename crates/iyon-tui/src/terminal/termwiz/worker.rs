use std::sync::mpsc::{Receiver, Sender, SyncSender};

use anyhow::{Context, Result, anyhow};
use termwiz::{
    caps::{Capabilities, ProbeHints},
    input::InputEvent,
    surface::{Change, CursorVisibility, Surface},
    terminal::{Terminal, TerminalWaker, new_terminal},
};

type EventSender = tokio::sync::mpsc::UnboundedSender<Result<crate::terminal::TerminalEvent>>;

type Reply<T> = Sender<Result<T>>;

pub(crate) enum TerminalCommand {
    Viewport {
        reply: Reply<crate::geometry::Size>,
    },
    Present {
        desired: Surface,
        reply: Reply<()>,
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
) {
    let setup = setup_terminal();
    let (mut terminal, waker, mut presenter) = match setup {
        Ok(value) => value,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };

    if startup.send(Ok(Startup { waker })).is_err() {
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

    let _ = restore_terminal(&mut *terminal);
}

fn setup_terminal() -> Result<(
    Box<dyn Terminal + Send>,
    TerminalWaker,
    super::presenter::TermwizPresenter,
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
    Ok((terminal, waker, presenter))
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
        TerminalCommand::Viewport { reply } => {
            let result = terminal
                .get_screen_size()
                .and_then(|size| {
                    presenter.resize(size.cols, size.rows);
                    Ok(crate::geometry::Size::new(
                        u16::try_from(size.cols)
                            .context("terminal width exceeds framework range")?,
                        u16::try_from(size.rows)
                            .context("terminal height exceeds framework range")?,
                    ))
                })
                .map_err(anyhow::Error::from);
            let _ = reply.send(result);
            false
        }
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
            let result = restore_terminal(terminal);
            let _ = reply.send(result);
            true
        }
    }
}

fn restore_terminal(terminal: &mut dyn Terminal) -> Result<()> {
    let mut first_error = None;
    if let Err(error) = terminal
        .render(&[Change::CursorVisibility(CursorVisibility::Visible)])
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
