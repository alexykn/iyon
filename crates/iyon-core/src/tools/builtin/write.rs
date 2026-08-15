use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use iyon_api::ContentBlock;
use serde::Deserialize;
use serde_json::{Value, json};
use similar::TextDiff;
use tokio::fs;

use crate::tools::{
    FileMutationQueue, ToolApprovalPolicy, ToolContext, ToolDefinition, ToolExecutionMode,
    ToolExecutor, ToolFuture, ToolResult, ToolSource, ToolUpdateSink,
};

#[derive(Debug)]
pub struct WriteTool {
    mutation_queue: FileMutationQueue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteInput {
    path: String,
    content: String,
}

impl WriteTool {
    pub fn new(mutation_queue: FileMutationQueue) -> Self {
        Self { mutation_queue }
    }
}

impl ToolExecutor for WriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write".to_string(),
            label: "write".to_string(),
            description: "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write (relative or absolute)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
            execution_mode: ToolExecutionMode::Sequential,
            approval: ToolApprovalPolicy::NeverAsk,
            source: ToolSource::Builtin,
            prompt_snippet: Some("Create or overwrite files".to_string()),
            prompt_guidelines: vec!["Use write only for new files or complete rewrites.".to_string()],
        }
    }

    fn execute(&self, ctx: ToolContext, input: Value, _updates: ToolUpdateSink) -> ToolFuture<'_> {
        let queue = self.mutation_queue.clone();
        Box::pin(async move {
            let input: WriteInput = serde_json::from_value(input).context("invalid write input")?;
            validate_input(&input)?;
            ensure_not_cancelled(&ctx)?;
            let path = ctx.workspace.resolve_write_path(&input.path)?;
            queue
                .run(path.clone(), || async move {
                    write_file(&ctx, path, input).await
                })
                .await
        })
    }
}

fn validate_input(input: &WriteInput) -> anyhow::Result<()> {
    if input.path.trim().is_empty() {
        bail!("write path must not be empty");
    }
    Ok(())
}

async fn write_file(
    ctx: &ToolContext,
    path: PathBuf,
    input: WriteInput,
) -> anyhow::Result<ToolResult> {
    ensure_not_cancelled(ctx)?;
    create_parent_dir(&path).await?;
    ensure_not_cancelled(ctx)?;
    let previous = existing_text(&path).await;
    fs::write(&path, input.content.as_bytes())
        .await
        .with_context(|| format!("failed to write file: {}", path.display()))?;
    ensure_not_cancelled(ctx)?;
    let after = normalize_to_lf(&input.content);
    Ok(ToolResult {
        content: vec![ContentBlock::Text {
            text: format!(
                "Successfully wrote {} bytes to {}",
                input.content.len(),
                input.path
            ),
        }],
        details: json!({
            "diff": generate_diff(&input.path, &previous, &after),
        }),
        is_error: false,
        terminate: false,
    })
}

async fn existing_text(path: &Path) -> String {
    match fs::read_to_string(path).await {
        Ok(text) => normalize_to_lf(&text),
        Err(_) => String::new(),
    }
}

fn normalize_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn generate_diff(path: &str, before: &str, after: &str) -> String {
    TextDiff::from_lines(before, after)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

async fn create_parent_dir(path: &std::path::Path) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent)
        .await
        .with_context(|| format!("failed to create parent directory: {}", parent.display()))
}

fn ensure_not_cancelled(ctx: &ToolContext) -> anyhow::Result<()> {
    if ctx.cancellation.is_cancelled() {
        bail!("write tool cancelled");
    }
    Ok(())
}
