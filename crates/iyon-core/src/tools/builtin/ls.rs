use std::fs;

use anyhow::{Context, bail};
use iyon_api::ContentBlock;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ToolApprovalPolicy, ToolContext, ToolDefinition, ToolExecutionMode, ToolExecutor, ToolFuture,
    ToolResult, ToolSource, ToolUpdateSink,
    output::{DEFAULT_MODEL_MAX_BYTES, ModelOutputLimits, truncate_head},
};

const DEFAULT_LIMIT: usize = 500;

#[derive(Debug, Default)]
pub struct LsTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LsInput {
    path: Option<String>,
    limit: Option<usize>,
}

impl ToolExecutor for LsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ls".to_string(),
            label: "ls".to_string(),
            description: format!(
                "List directory contents. Returns entries sorted alphabetically, with '/' suffix for directories. Includes dotfiles. Output is truncated to {DEFAULT_LIMIT} entries or {}KB (whichever is hit first).",
                DEFAULT_MODEL_MAX_BYTES / 1024
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory to list (default: current directory)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of entries to return (default: 500)"
                    }
                },
                "additionalProperties": false
            }),
            execution_mode: ToolExecutionMode::Parallel,
            approval: ToolApprovalPolicy::NeverAsk,
            source: ToolSource::Builtin,
            prompt_snippet: Some("List directory contents".to_string()),
            prompt_guidelines: Vec::new(),
        }
    }

    fn execute(&self, ctx: ToolContext, input: Value, _updates: ToolUpdateSink) -> ToolFuture<'_> {
        Box::pin(async move {
            let input: LsInput = serde_json::from_value(input).context("invalid ls input")?;
            ensure_not_cancelled(&ctx)?;
            let dir = ctx.workspace.resolve_search_path(input.path.as_deref())?;
            let limit = input.limit.unwrap_or(DEFAULT_LIMIT).max(1);
            let entries = read_entries(&dir, limit)?;
            ensure_not_cancelled(&ctx)?;
            Ok(build_result(entries, limit))
        })
    }
}

struct LsEntries {
    entries: Vec<String>,
    limit_reached: bool,
}

fn read_entries(dir: &std::path::Path, limit: usize) -> anyhow::Result<LsEntries> {
    let metadata =
        fs::metadata(dir).with_context(|| format!("failed to stat path: {}", dir.display()))?;
    if !metadata.is_dir() {
        bail!("not a directory: {}", dir.display());
    }

    let mut names = fs::read_dir(dir)
        .with_context(|| format!("cannot read directory: {}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    names.sort_by(|a, b| {
        a.file_name()
            .to_string_lossy()
            .to_lowercase()
            .cmp(&b.file_name().to_string_lossy().to_lowercase())
    });

    let mut entries = Vec::new();
    let mut limit_reached = false;
    for entry in names {
        if entries.len() >= limit {
            limit_reached = true;
            break;
        }
        let mut name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type().is_ok_and(|ty| ty.is_dir()) {
            name.push('/');
        }
        entries.push(name);
    }

    Ok(LsEntries {
        entries,
        limit_reached,
    })
}

fn build_result(entries: LsEntries, limit: usize) -> ToolResult {
    if entries.entries.is_empty() {
        return text_result("(empty directory)".to_string(), json!({}));
    }

    let raw = entries.entries.join("\n");
    let truncated = truncate_head(
        &raw,
        ModelOutputLimits {
            max_lines: usize::MAX / 2,
            max_bytes: DEFAULT_MODEL_MAX_BYTES,
        },
    );
    let mut text = truncated.text;
    let mut details = serde_json::Map::new();
    let mut notices = Vec::new();

    if entries.limit_reached {
        details.insert("entryLimitReached".to_string(), json!(limit));
        notices.push(format!(
            "{limit} entries limit reached. Use limit={} for more",
            limit.saturating_mul(2)
        ));
    }
    if truncated.report.truncated {
        details.insert("truncation".to_string(), json!(truncated.report));
        notices.push(format!(
            "{}KB limit reached",
            DEFAULT_MODEL_MAX_BYTES / 1024
        ));
    }
    if !notices.is_empty() {
        text.push_str(&format!("\n\n[{}]", notices.join(". ")));
    }

    text_result(text, Value::Object(details))
}

fn text_result(text: String, details: Value) -> ToolResult {
    ToolResult {
        content: vec![ContentBlock::Text { text }],
        details,
        is_error: false,
        terminate: false,
    }
}

fn ensure_not_cancelled(ctx: &ToolContext) -> anyhow::Result<()> {
    if ctx.cancellation.is_cancelled() {
        bail!("ls tool cancelled");
    }
    Ok(())
}
