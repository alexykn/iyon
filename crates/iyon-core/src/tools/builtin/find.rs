use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use iyon_api::ContentBlock;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ToolApprovalPolicy, ToolContext, ToolDefinition, ToolExecutionMode, ToolExecutor, ToolFuture,
    ToolResult, ToolSource, ToolUpdateSink,
    output::{DEFAULT_MODEL_MAX_BYTES, ModelOutputLimits, truncate_head},
    process::{ProcessSpec, find_program, run_capture},
};

const DEFAULT_LIMIT: usize = 1_000;

#[derive(Debug, Default)]
pub struct FindTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindInput {
    pattern: String,
    path: Option<String>,
    limit: Option<usize>,
}

impl ToolExecutor for FindTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "find".to_string(),
            label: "find".to_string(),
            description: format!(
                "Search for files by glob pattern. Prefers fd and falls back to find. Output is truncated to {DEFAULT_LIMIT} results or {}KB (whichever is hit first).",
                DEFAULT_MODEL_MAX_BYTES / 1024
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match files, e.g. '*.ts', '**/*.json', or 'src/**/*.spec.ts'"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (default: current directory)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results (default: 1000)"
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            execution_mode: ToolExecutionMode::Parallel,
            approval: ToolApprovalPolicy::NeverAsk,
            source: ToolSource::Builtin,
            prompt_snippet: Some("Find files by glob pattern".to_string()),
            prompt_guidelines: Vec::new(),
        }
    }

    fn execute(&self, ctx: ToolContext, input: Value, _updates: ToolUpdateSink) -> ToolFuture<'_> {
        Box::pin(async move {
            let input: FindInput = serde_json::from_value(input).context("invalid find input")?;
            validate_input(&input)?;
            let search_root = ctx.workspace.resolve_search_path(input.path.as_deref())?;
            let limit = input.limit.unwrap_or(DEFAULT_LIMIT).max(1);
            let output = run_find_command(&ctx, &input.pattern, &search_root, limit).await?;
            Ok(build_result(output, &search_root, limit))
        })
    }
}

struct FindOutput {
    paths: Vec<String>,
    limit_reached: bool,
}

fn validate_input(input: &FindInput) -> anyhow::Result<()> {
    if input.pattern.trim().is_empty() {
        bail!("find pattern must not be empty");
    }
    Ok(())
}

async fn run_find_command(
    ctx: &ToolContext,
    pattern: &str,
    search_root: &Path,
    limit: usize,
) -> anyhow::Result<FindOutput> {
    if let Some(fd) = find_program("fd") {
        return run_fd(ctx, fd, pattern, search_root, limit).await;
    }
    if let Some(find) = find_program("find") {
        return run_system_find(ctx, find, pattern, search_root, limit).await;
    }
    bail!("neither fd nor find is available")
}

async fn run_fd(
    ctx: &ToolContext,
    fd: PathBuf,
    pattern: &str,
    search_root: &Path,
    limit: usize,
) -> anyhow::Result<FindOutput> {
    let mut args = vec![
        "--glob".to_string(),
        "--color=never".to_string(),
        "--hidden".to_string(),
        "--no-require-git".to_string(),
        "--max-results".to_string(),
        limit.to_string(),
    ];
    let mut effective_pattern = pattern.to_string();
    if pattern.contains('/') {
        args.push("--full-path".to_string());
        if !pattern.starts_with('/') && !pattern.starts_with("**/") && pattern != "**" {
            effective_pattern = format!("**/{pattern}");
        }
    }
    args.extend([
        "--".to_string(),
        effective_pattern,
        search_root.display().to_string(),
    ]);

    let output = run_capture(
        ProcessSpec {
            program: fd,
            args,
            cwd: ctx.cwd.clone(),
            timeout: None,
            merge_stderr: false,
        },
        ctx.cancellation.clone(),
    )
    .await?;
    if output.exit_code.is_some_and(|code| code != 0) && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "fd failed: {}",
            if stderr.is_empty() {
                "unknown error"
            } else {
                &stderr
            }
        );
    }
    Ok(parse_paths(
        &String::from_utf8_lossy(&output.stdout),
        search_root,
        limit,
    ))
}

async fn run_system_find(
    ctx: &ToolContext,
    find: PathBuf,
    pattern: &str,
    search_root: &Path,
    limit: usize,
) -> anyhow::Result<FindOutput> {
    let matcher = if pattern.contains('/') {
        "-path"
    } else {
        "-name"
    };
    let effective_pattern = if pattern.contains('/') {
        format!("*/{pattern}")
    } else {
        pattern.to_string()
    };
    let args = vec![
        search_root.display().to_string(),
        "-path".to_string(),
        "*/.git".to_string(),
        "-prune".to_string(),
        "-o".to_string(),
        "-path".to_string(),
        "*/node_modules".to_string(),
        "-prune".to_string(),
        "-o".to_string(),
        matcher.to_string(),
        effective_pattern,
        "-print".to_string(),
    ];
    let output = run_capture(
        ProcessSpec {
            program: find,
            args,
            cwd: ctx.cwd.clone(),
            timeout: None,
            merge_stderr: false,
        },
        ctx.cancellation.clone(),
    )
    .await?;
    if output.exit_code.is_some_and(|code| code != 0) && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "find failed: {}",
            if stderr.is_empty() {
                "unknown error"
            } else {
                &stderr
            }
        );
    }
    Ok(parse_paths(
        &String::from_utf8_lossy(&output.stdout),
        search_root,
        limit,
    ))
}

fn parse_paths(output: &str, search_root: &Path, limit: usize) -> FindOutput {
    let mut paths = Vec::new();
    let mut limit_reached = false;
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if paths.len() >= limit {
            limit_reached = true;
            break;
        }
        paths.push(to_relative_posix_path(line, search_root));
    }
    if paths.len() >= limit {
        limit_reached = true;
    }
    FindOutput {
        paths,
        limit_reached,
    }
}

fn to_relative_posix_path(path: &str, search_root: &Path) -> String {
    let had_trailing_slash = path.ends_with('/') || path.ends_with('\\');
    let path = Path::new(path);
    let relative = path.strip_prefix(search_root).unwrap_or(path);
    let mut text = relative.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        text = ".".to_string();
    }
    if had_trailing_slash && !text.ends_with('/') {
        text.push('/');
    }
    text
}

fn build_result(output: FindOutput, _search_root: &Path, limit: usize) -> ToolResult {
    if output.paths.is_empty() {
        return text_result("No files found matching pattern".to_string(), json!({}));
    }

    let raw = output.paths.join("\n");
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

    if output.limit_reached {
        details.insert("resultLimitReached".to_string(), json!(limit));
        notices.push(format!(
            "{limit} results limit reached. Use limit={} for more, or refine pattern",
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
