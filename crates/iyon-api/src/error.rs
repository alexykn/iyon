use std::{error::Error, fmt};

#[derive(Debug, Clone)]
pub struct ModelError {
    pub kind: ModelErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelErrorKind {
    InvalidRequest,
    Authentication,
    RateLimited,
    Provider,
    Transport,
    Cancelled,
    Unknown,
}

impl ModelError {
    pub fn new(kind: ModelErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self::new(ModelErrorKind::Unknown, message)
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for ModelError {}
