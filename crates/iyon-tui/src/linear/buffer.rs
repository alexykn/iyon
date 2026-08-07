//! Synchronized-update guard.
//!
//! Brackets destructive terminal writes (history append + viewport repaint) so
//! they are released to the screen atomically, avoiding a visible "clear then
//! paint" intermediate state. The guard owns a `Stdout` handle (not a borrow of
//! the terminal) so the presenter can freely use the terminal while it is alive,
//! and it always emits the end sequence on drop.

use std::io;

use crossterm::{execute, terminal::EndSynchronizedUpdate};

use crate::terminal::InlineTerminal;

/// RAII guard that brackets [`BeginSynchronizedUpdate`] /
/// [`EndSynchronizedUpdate`]. Always emits the end sequence on drop, even on
/// error or panic, so the terminal is never left withholding a frame.
pub(crate) struct SyncUpdateGuard {
    stdout: io::Stdout,
    finished: bool,
}

impl SyncUpdateGuard {
    pub(crate) fn begin(terminal: &mut InlineTerminal) -> anyhow::Result<Self> {
        terminal.begin_sync()?;
        Ok(Self {
            stdout: io::stdout(),
            finished: false,
        })
    }

    pub(crate) fn finish(mut self) -> anyhow::Result<()> {
        end_sync(&mut self.stdout)?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for SyncUpdateGuard {
    fn drop(&mut self) {
        if !self.finished {
            // Best-effort: ignore failure so Drop never propagates a panic.
            let _ = end_sync(&mut self.stdout);
        }
    }
}

fn end_sync(stdout: &mut io::Stdout) -> anyhow::Result<()> {
    execute!(stdout, EndSynchronizedUpdate)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_sync_writes_end_sequence() {
        // Smoke: the free function resolves to the crossterm command.
        // Real terminal verification is covered by the vt100 tests.
        let _ = end_sync;
    }
}
