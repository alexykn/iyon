use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TurnId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolCallId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ApprovalId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueueItemId(pub u64);

impl From<TurnId> for u64 {
    fn from(id: TurnId) -> Self {
        id.0
    }
}

impl From<MessageId> for u64 {
    fn from(id: MessageId) -> Self {
        id.0
    }
}

impl From<QueueItemId> for u64 {
    fn from(id: QueueItemId) -> Self {
        id.0
    }
}

impl fmt::Display for ToolCallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
