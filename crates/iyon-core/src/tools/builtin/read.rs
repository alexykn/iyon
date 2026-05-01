use anyhow::{anyhow, bail};
use iyon_api::ContentBlock;
use serde_json::{Value, json};

use crate::{
    fs::read::read_text_file,
    tools::{
        ToolApprovalPolicy, ToolContext, ToolDefinition, ToolExecutionMode, ToolExecutor,
        ToolFuture, ToolResult, ToolSource, ToolUpdateSink,
    },
};

#[derive(Debug, Default)]
pub struct ReadTool;

impl ToolExecutor for ReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read".to_string(),
            label: "Read".to_string(),
            description: "Read a UTF-8 text file from the workspace.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read, relative to the workspace root."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            execution_mode: ToolExecutionMode::Parallel,
            approval: ToolApprovalPolicy::NeverAsk,
            source: ToolSource::Builtin,
            prompt_snippet: Some("read: Read a UTF-8 text file from the workspace.".to_string()),
            prompt_guidelines: Vec::new(),
        }
    }

    fn execute(&self, ctx: ToolContext, input: Value, _updates: ToolUpdateSink) -> ToolFuture<'_> {
        Box::pin(async move {
            let path = input
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("read tool requires string field: path"))?;
            if path.trim().is_empty() {
                bail!("read tool path must not be empty");
            }

            if ctx.cancellation.is_cancelled() {
                bail!("read tool cancelled");
            }
            let text = read_text_file(&ctx.workspace, path)?;
            Ok(ToolResult {
                content: vec![ContentBlock::Text { text }],
                details: json!({ "path": path }),
                is_error: false,
                terminate: false,
            })
        })
    }
}
