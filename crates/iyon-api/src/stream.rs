#[derive(Debug, Clone)]
pub enum ModelStreamEvent {
    Started,
    TextDelta { delta: String },
    Done { stop_reason: StopReason },
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}
