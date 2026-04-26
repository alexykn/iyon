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

    fn execute<'a>(
        &'a self,
        ctx: ToolContext,
        input: Value,
        _updates: ToolUpdateSink,
    ) -> ToolFuture<'a> {
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

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::SystemTime};

    use serde_json::json;

    use super::*;
    use crate::{
        fs::{FsPermissions, Workspace},
        ids::{SessionId, ToolCallId, TurnId},
        tools::ToolUpdateSink,
    };

    #[tokio::test]
    async fn read_tool_reads_workspace_file() {
        let root = create_temp_dir("read-tool");
        fs::write(root.join("file.txt"), "hello").unwrap();
        let tool = ReadTool;
        let ctx = ToolContext {
            session_id: SessionId(1),
            turn_id: TurnId(1),
            tool_call_id: ToolCallId("call-1".to_string()),
            cwd: root.clone(),
            workspace: Workspace::new(root.clone(), FsPermissions::default()),
            cancellation: tokio_util::sync::CancellationToken::new(),
        };

        let result = tool
            .execute(ctx, json!({ "path": "file.txt" }), ToolUpdateSink::noop())
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(matches!(
            &result.content[..],
            [ContentBlock::Text { text }] if text == "hello"
        ));
        let _ = fs::remove_dir_all(root);
    }

    fn create_temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "iyon-{prefix}-{}",
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
