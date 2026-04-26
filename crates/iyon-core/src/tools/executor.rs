#![allow(dead_code)]

use std::{future::Future, path::PathBuf, pin::Pin};

use anyhow::bail;
use iyon_api::ContentBlock;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    CoreEvent, ToolUpdateEvent,
    fs::Workspace,
    ids::{MessageId, SessionId, ToolCallId, TurnId},
    tools::definition::ToolDefinition,
};

pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<ToolResult>> + Send + 'a>>;

pub trait ToolExecutor: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    fn execute<'a>(
        &'a self,
        ctx: ToolContext,
        input: Value,
        updates: ToolUpdateSink,
    ) -> ToolFuture<'a>;
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub tool_call_id: ToolCallId,
    pub cwd: PathBuf,
    pub workspace: Workspace,
    pub cancellation: CancellationToken,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    pub details: Value,
    pub is_error: bool,
    pub terminate: bool,
}

#[derive(Debug, Clone)]
pub enum ToolUpdate {
    Text(String),
    Progress {
        label: String,
        current: Option<u64>,
        total: Option<u64>,
    },
    Details(Value),
}

#[derive(Debug, Clone)]
pub struct ToolUpdateSink {
    event_tx: mpsc::Sender<CoreEvent>,
    turn_id: TurnId,
    message_id: MessageId,
    tool_call_id: ToolCallId,
    tool_name: String,
    cancellation: CancellationToken,
}

impl ToolUpdateSink {
    pub(crate) fn new(
        event_tx: mpsc::Sender<CoreEvent>,
        turn_id: TurnId,
        message_id: MessageId,
        tool_call_id: ToolCallId,
        tool_name: String,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            event_tx,
            turn_id,
            message_id,
            tool_call_id,
            tool_name,
            cancellation,
        }
    }

    pub(crate) fn noop() -> Self {
        let (event_tx, _event_rx) = mpsc::channel(1);
        Self::new(
            event_tx,
            TurnId(0),
            MessageId(0),
            ToolCallId(String::new()),
            String::new(),
            CancellationToken::new(),
        )
    }

    pub async fn send(&self, update: ToolUpdate) -> anyhow::Result<()> {
        if self.cancellation.is_cancelled() {
            bail!("tool update cancelled");
        }

        self.event_tx
            .send(CoreEvent::ToolCallUpdated {
                turn_id: self.turn_id.0,
                message_id: self.message_id.0,
                tool_call_id: self.tool_call_id.0.clone(),
                tool_name: self.tool_name.clone(),
                update: lower_tool_update(update),
            })
            .await?;
        Ok(())
    }
}

impl Default for ToolUpdateSink {
    fn default() -> Self {
        Self::noop()
    }
}

fn lower_tool_update(update: ToolUpdate) -> ToolUpdateEvent {
    match update {
        ToolUpdate::Text(text) => ToolUpdateEvent::Text(text),
        ToolUpdate::Progress {
            label,
            current,
            total,
        } => ToolUpdateEvent::Progress {
            label,
            current,
            total,
        },
        ToolUpdate::Details(details) => ToolUpdateEvent::Details(details),
    }
}
