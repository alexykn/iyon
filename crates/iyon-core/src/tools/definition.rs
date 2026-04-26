#![allow(dead_code)]

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub label: String,
    pub description: String,
    pub input_schema: Value,
    pub execution_mode: ToolExecutionMode,
    pub approval: ToolApprovalPolicy,
    pub source: ToolSource,
    pub prompt_snippet: Option<String>,
    pub prompt_guidelines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    Sequential,
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApprovalPolicy {
    NeverAsk,
    AlwaysAsk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
    Builtin,
    Extension { extension_id: String },
    Sdk,
}
