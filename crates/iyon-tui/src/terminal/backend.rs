use anyhow::Result;

use crate::{backend::NativeHistorySink, geometry::Size, scene::PreparedSceneFrame};

/// Semantic terminal input understood by the runtime driver.
#[derive(Debug)]
pub(crate) enum TerminalEvent {
    Key(crate::KeyStroke),
    Paste(String),
    Resize,
}

/// Private semantic terminal session boundary.
///
/// `next_event` is cancellation-safe: the runtime may stop awaiting it during
/// a select cycle and await the next event again without losing backend state.
pub(crate) trait TerminalBackend: NativeHistorySink<Error = anyhow::Error> {
    async fn next_event(&mut self) -> Result<TerminalEvent>;

    fn try_next_event(&mut self) -> Result<Option<TerminalEvent>>;

    fn viewport(&mut self) -> Result<Size>;

    fn draw_frame(&mut self, frame: &PreparedSceneFrame) -> Result<()>;

    fn position_after_final_frame(&mut self) -> Result<()>;

    fn restore(&mut self) -> Result<()>;
}
