#![allow(dead_code)]

use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::Result;
use iyon_api::ContentBlock;
use serde_json::Value;

use crate::{
    ids::{ToolCallId, TurnId},
    session::state::SessionState,
    tools::ToolResult,
};

pub type BeforeHookFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BeforeToolCallDecision>> + Send + 'a>>;
pub type AfterHookFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<AfterToolCallPatch>>> + Send + 'a>>;

pub trait BeforeToolCallHook: Send + Sync {
    fn before_tool_call<'a>(&'a self, ctx: BeforeToolCallContext<'a>) -> BeforeHookFuture<'a>;
}

pub trait AfterToolCallHook: Send + Sync {
    fn after_tool_call<'a>(&'a self, ctx: AfterToolCallContext<'a>) -> AfterHookFuture<'a>;
}

#[derive(Clone, Default)]
pub struct ToolHookSet {
    before: Vec<Arc<dyn BeforeToolCallHook>>,
    after: Vec<Arc<dyn AfterToolCallHook>>,
}

#[derive(Clone, Default)]
pub struct ToolHookSnapshot {
    before: Vec<Arc<dyn BeforeToolCallHook>>,
    after: Vec<Arc<dyn AfterToolCallHook>>,
}

pub struct BeforeToolCallContext<'a> {
    pub turn_id: TurnId,
    pub tool_call_id: &'a ToolCallId,
    pub tool_name: &'a str,
    pub args: &'a Value,
    pub session: &'a SessionState,
}

pub enum BeforeToolCallDecision {
    Allow,
    Block { reason: Option<String> },
}

pub struct AfterToolCallContext<'a> {
    pub turn_id: TurnId,
    pub tool_call_id: &'a ToolCallId,
    pub tool_name: &'a str,
    pub args: &'a Value,
    pub result: &'a ToolResult,
    pub session: &'a SessionState,
}

#[derive(Default)]
pub struct AfterToolCallPatch {
    pub content_override: Option<Vec<ContentBlock>>,
    pub details_override: Option<Value>,
    pub is_error_override: Option<bool>,
    pub terminate_override: Option<bool>,
}

impl ToolHookSet {
    pub fn register_before(&mut self, hook: Arc<dyn BeforeToolCallHook>) {
        self.before.push(hook);
    }

    pub fn register_before_fn<F>(&mut self, hook: F)
    where
        F: BeforeToolCallHook + 'static,
    {
        self.before.push(Arc::new(hook));
    }

    pub fn register_after(&mut self, hook: Arc<dyn AfterToolCallHook>) {
        self.after.push(hook);
    }

    pub fn register_after_fn<F>(&mut self, hook: F)
    where
        F: AfterToolCallHook + 'static,
    {
        self.after.push(Arc::new(hook));
    }

    #[must_use]
    pub fn snapshot(&self) -> ToolHookSnapshot {
        ToolHookSnapshot {
            before: self.before.clone(),
            after: self.after.clone(),
        }
    }
}

impl ToolHookSnapshot {
    pub async fn run_before_hooks(
        &self,
        ctx: BeforeToolCallContext<'_>,
    ) -> Result<BeforeToolCallDecision> {
        for hook in &self.before {
            match hook
                .before_tool_call(BeforeToolCallContext { ..ctx })
                .await?
            {
                BeforeToolCallDecision::Allow => {}
                decision @ BeforeToolCallDecision::Block { .. } => return Ok(decision),
            }
        }
        Ok(BeforeToolCallDecision::Allow)
    }

    pub async fn run_after_hooks(
        &self,
        ctx: AfterToolCallContext<'_>,
    ) -> Result<AfterToolCallPatch> {
        let mut merged = AfterToolCallPatch::default();
        for hook in &self.after {
            if let Some(patch) = hook.after_tool_call(AfterToolCallContext { ..ctx }).await? {
                if patch.content_override.is_some() {
                    merged.content_override = patch.content_override;
                }
                if patch.details_override.is_some() {
                    merged.details_override = patch.details_override;
                }
                if patch.is_error_override.is_some() {
                    merged.is_error_override = patch.is_error_override;
                }
                if patch.terminate_override.is_some() {
                    merged.terminate_override = patch.terminate_override;
                }
            }
        }
        Ok(merged)
    }
}
