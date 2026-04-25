#[derive(Debug, Clone)]
pub enum CoreEvent {
    TurnStarted { turn_id: u64 },
    MessageDelta { turn_id: u64, delta: MessageDelta },
    TurnFinished { turn_id: u64 },
    TurnFailed { turn_id: u64, message: String },
    TurnCancelled { turn_id: u64 },
}

#[derive(Debug, Clone)]
pub enum MessageDelta {
    Text(String),
}
