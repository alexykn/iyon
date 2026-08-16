use std::time::SystemTime;

use iyon_api::{ContentBlock, ModelMessage, StopReason, Usage};
use serde_json::Value;
use thiserror::Error;

use crate::ids::{MessageId, SessionId, ToolCallId};

pub use crate::agent::transcript::AgentMessage;

#[derive(Debug, Clone)]
pub enum SessionEntry {
    Message(AgentMessage),
    Custom { namespace: String, data: Value },
}

#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub entries: Vec<SessionEntry>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("invalid custom entry namespace: {namespace}")]
pub struct CustomEntryError {
    pub namespace: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SessionMessageError {
    #[error("message id {0:?} is already present in session")]
    ConflictingMessageId(MessageId),
    #[error("message id allocation exhausted")]
    IdExhausted,
    #[error(transparent)]
    InvalidNamespace(#[from] CustomEntryError),
}

#[derive(Debug, Clone)]
pub struct KernelSession {
    id: SessionId,
    next_message_id: u64,
    entries: Vec<SessionEntry>,
}

impl KernelSession {
    pub fn new(id: SessionId) -> Self {
        Self {
            id,
            next_message_id: 1,
            entries: Vec::new(),
        }
    }

    pub fn with_next_message_id(id: SessionId, next_message_id: u64) -> Self {
        Self {
            id,
            next_message_id: next_message_id.max(1),
            entries: Vec::new(),
        }
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn entries(&self) -> &[SessionEntry] {
        &self.entries
    }

    pub fn messages(&self) -> Vec<&AgentMessage> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                SessionEntry::Message(message)
                    if !matches!(message, AgentMessage::Status { .. }) =>
                {
                    Some(message)
                }
                _ => None,
            })
            .collect()
    }

    pub fn append_message(
        &mut self,
        mut message: AgentMessage,
    ) -> Result<MessageId, SessionMessageError> {
        let id = message.id();
        let id = if id.0 == 0 {
            let id = self.allocate_message_id()?;
            message.set_id(id);
            id
        } else {
            if self
                .entries
                .iter()
                .any(|entry| entry.message_id() == Some(id))
            {
                return Err(SessionMessageError::ConflictingMessageId(id));
            }
            self.next_message_id = self.next_message_id.max(id.0.saturating_add(1));
            id
        };

        self.entries.push(SessionEntry::Message(message));
        Ok(id)
    }

    pub fn append_entry(&mut self, entry: SessionEntry) -> Result<(), SessionMessageError> {
        if let SessionEntry::Custom { namespace, .. } = &entry {
            validate_namespace(namespace)?;
        }
        match entry {
            SessionEntry::Message(message) => self.append_message(message).map(|_| ()),
            custom => {
                self.entries.push(custom);
                Ok(())
            }
        }
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session_id: self.id,
            entries: self.entries.clone(),
        }
    }

    pub fn to_model_messages(&self) -> Vec<ModelMessage> {
        self.messages()
            .into_iter()
            .filter_map(lower_message)
            .collect()
    }

    pub fn next_message_id(&self) -> u64 {
        self.next_message_id
    }

    pub fn from_legacy(session: &crate::session::state::SessionState) -> Self {
        let mut kernel = Self::with_next_message_id(session.id, 1);
        for message in &session.messages {
            // The compatibility state is already validated.  Keeping this
            // conversion infallible preserves its existing public behavior.
            let _ = kernel.append_message(message.clone());
        }
        kernel
    }

    pub fn sync_to_legacy(&self, session: &mut crate::session::state::SessionState) {
        session.messages = self.messages().into_iter().cloned().collect();
    }

    fn allocate_message_id(&mut self) -> Result<MessageId, SessionMessageError> {
        let id = MessageId(self.next_message_id);
        self.next_message_id = self
            .next_message_id
            .checked_add(1)
            .ok_or(SessionMessageError::IdExhausted)?;
        Ok(id)
    }
}

impl SessionEntry {
    fn message_id(&self) -> Option<MessageId> {
        match self {
            Self::Message(message) => Some(message.id()),
            Self::Custom { .. } => None,
        }
    }
}

fn validate_namespace(namespace: &str) -> Result<(), CustomEntryError> {
    let valid = !namespace.is_empty()
        && namespace.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().enumerate().all(|(index, byte)| {
                    byte.is_ascii_alphanumeric() || byte == b'_' || (index > 0 && byte == b'-')
                })
                && !segment.as_bytes()[0].is_ascii_digit()
        });
    if valid {
        Ok(())
    } else {
        Err(CustomEntryError {
            namespace: namespace.to_string(),
        })
    }
}

fn lower_message(message: &AgentMessage) -> Option<ModelMessage> {
    match message {
        AgentMessage::User { content, .. } => Some(ModelMessage::User {
            content: content.clone(),
        }),
        AgentMessage::Assistant { content, .. } => Some(ModelMessage::Assistant {
            content: content.clone(),
        }),
        AgentMessage::ToolResult {
            tool_call_id,
            tool_name,
            content,
            is_error,
            ..
        } => Some(ModelMessage::ToolResult {
            tool_call_id: tool_call_id.0.clone(),
            tool_name: tool_name.clone(),
            content: content.clone(),
            is_error: *is_error,
        }),
        AgentMessage::Status { .. } => None,
    }
}

impl AgentMessage {
    pub fn user(content: Vec<ContentBlock>) -> Self {
        Self::User {
            id: MessageId(0),
            content,
            timestamp: SystemTime::now(),
        }
    }

    pub fn assistant(
        content: Vec<ContentBlock>,
        usage: Option<Usage>,
        stop_reason: Option<StopReason>,
    ) -> Self {
        Self::Assistant {
            id: MessageId(0),
            content,
            usage,
            stop_reason,
            timestamp: SystemTime::now(),
        }
    }

    pub fn tool_result(
        tool_call_id: ToolCallId,
        tool_name: String,
        content: Vec<ContentBlock>,
        details: Value,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            id: MessageId(0),
            tool_call_id,
            tool_name,
            content,
            details,
            is_error,
            timestamp: SystemTime::now(),
        }
    }

    fn set_id(&mut self, id: MessageId) {
        match self {
            Self::User { id: current, .. }
            | Self::Assistant { id: current, .. }
            | Self::ToolResult { id: current, .. }
            | Self::Status { id: current, .. } => *current = id,
        }
    }
}

#[cfg(test)]
mod tests {
    use iyon_api::{ContentBlock, ModelMessage};
    use serde_json::json;

    use super::{AgentMessage, KernelSession, SessionEntry, SessionMessageError};
    use crate::{
        ids::{MessageId, SessionId, ToolCallId},
        session::state::SessionState,
    };

    fn user(text: &str) -> AgentMessage {
        AgentMessage::user(vec![ContentBlock::Text {
            text: text.to_string(),
        }])
    }

    #[test]
    fn append_message_allocates_native_id() {
        let mut session = KernelSession::new(SessionId(7));
        let id = session.append_message(user("hello")).unwrap();

        assert_eq!(id, MessageId(1));
        assert_eq!(session.messages()[0].id(), id);
        assert_eq!(session.next_message_id(), 2);
    }

    #[test]
    fn append_entry_rejects_invalid_namespace() {
        let mut session = KernelSession::new(SessionId(1));
        let error = session
            .append_entry(SessionEntry::Custom {
                namespace: "../unsafe".to_string(),
                data: json!({}),
            })
            .unwrap_err();

        assert!(matches!(error, SessionMessageError::InvalidNamespace(_)));
    }

    #[test]
    fn snapshot_preserves_custom_entries() {
        let mut session = KernelSession::new(SessionId(1));
        session
            .append_entry(SessionEntry::Custom {
                namespace: "acme.trace".to_string(),
                data: json!({"span": 3}),
            })
            .unwrap();

        let snapshot = session.snapshot();
        assert!(
            matches!(snapshot.entries[0], SessionEntry::Custom { ref namespace, .. } if namespace == "acme.trace")
        );
    }

    #[test]
    fn messages_query_excludes_custom_and_status_entries() {
        let mut session = KernelSession::new(SessionId(1));
        session.append_message(user("hello")).unwrap();
        session
            .append_entry(SessionEntry::Message(AgentMessage::Status {
                id: MessageId(0),
                text: "working".to_string(),
                timestamp: std::time::SystemTime::now(),
            }))
            .unwrap();
        session
            .append_entry(SessionEntry::Custom {
                namespace: "acme.trace".to_string(),
                data: json!({}),
            })
            .unwrap();

        assert_eq!(session.messages().len(), 1);
    }

    #[test]
    fn to_model_messages_does_not_mutate_canonical_entries() {
        let mut session = KernelSession::new(SessionId(1));
        session.append_message(user("hello")).unwrap();
        session
            .append_entry(SessionEntry::Custom {
                namespace: "acme.trace".to_string(),
                data: json!({"kept": true}),
            })
            .unwrap();
        let before = session.snapshot();

        let projection = session.to_model_messages();

        assert!(matches!(projection[0], ModelMessage::User { .. }));
        assert_eq!(session.snapshot().entries.len(), before.entries.len());
        assert!(matches!(session.entries()[1], SessionEntry::Custom { .. }));
    }

    #[test]
    fn compatibility_session_state_round_trips_without_losing_metadata() {
        let mut state = SessionState::new(SessionId(1), ".".into());
        state.metadata.user_id = Some("user-1".to_string());
        state.messages.push(AgentMessage::ToolResult {
            id: MessageId(4),
            tool_call_id: ToolCallId("call-1".to_string()),
            tool_name: "read".to_string(),
            content: vec![ContentBlock::Text {
                text: "ok".to_string(),
            }],
            details: json!({"bytes": 2}),
            is_error: false,
            timestamp: std::time::SystemTime::now(),
        });

        let kernel = KernelSession::from_legacy(&state);
        let mut restored = state.clone();
        kernel.sync_to_legacy(&mut restored);

        assert_eq!(restored.metadata.user_id, Some("user-1".to_string()));
        assert_eq!(restored.messages.len(), 1);
        assert_eq!(restored.messages[0].id(), MessageId(4));
    }
}
