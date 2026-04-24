#[derive(Debug, Clone)]
pub enum CoreCommand {
    SubmitTurn { text: String },
    CancelActiveTurn,
    Shutdown,
}
