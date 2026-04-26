#![allow(dead_code)]

use std::path::PathBuf;

use crate::{agent::transcript::AgentMessage, ids::SessionId};

#[derive(Debug, Clone)]
pub struct SessionState {
    pub id: SessionId,
    pub cwd: PathBuf,
    pub messages: Vec<AgentMessage>,
    pub model: ModelSelection,
    pub system_prompt: String,
    pub metadata: SessionMetadata,
}

#[derive(Debug, Clone)]
pub struct ModelSelection {
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct SessionMetadata {
    pub user_id: Option<String>,
}

impl SessionState {
    pub fn new(id: SessionId, cwd: PathBuf) -> Self {
        Self {
            id,
            cwd,
            messages: Vec::new(),
            model: ModelSelection {
                provider: "mock".to_string(),
                model_id: "mock".to_string(),
            },
            system_prompt: String::new(),
            metadata: SessionMetadata::default(),
        }
    }
}
