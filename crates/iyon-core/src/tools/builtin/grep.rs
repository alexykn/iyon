use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use iyon_api::ContentBlock;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ToolApprovalPolicy, ToolContext, ToolDefinition, ToolExecutionMode, ToolExecutor, ToolFuture,
    ToolResult, ToolSource, ToolUpdateSink,
    output::{
        DEFAULT_MODEL_MAX_BYTES, GREP_MAX_LINE_CHARS, ModelOutputLimits, truncate_head,
        truncate_line,
    },
    process::{ProcessSpec, find_program, run_capture},
};

const DEFAULT_LIMIT: usize = 100;

#[derive(Debug, Default)]
pub struct GrepTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GrepInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
    ignore_case: Option<bool>,
    literal: Option<bool>,
    context: Option<usize>,
    limit: Option<usize>,
}

impl ToolExecutor for GrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".to_string(),
            label: "grep".to_string(),
            description: format!(
                "Search file contents for a pattern. Prefers ripgrep and falls back to grep. Returns matching lines with file paths and line numbers. Output is truncated to {DEFAULT_LIMIT} matches or {}KB (whichever is hit first). Long lines are truncated to {GREP_MAX_LINE_CHARS} chars.",
                DEFAULT_MODEL_MAX_BYTES / 1024
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex or literal string)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file to search (default: current directory)"
                    },
                    "glob": {
                        "type": "string",
                        "description": "Filter files by glob pattern, e.g. '*.ts' or '**/*.spec.ts'"
                    },
                    "ignoreCase": {
                        "type": "boolean",
                        "description": "Case-insensitive search (default: false)"
                    },
                    "literal": {
                        "type": "boolean",
                        "description": "Treat pattern as literal string instead of regex (default: false)"
                    },
                    "context": {
                        "type": "number",
                        "description": "Number of lines to show before and after each match (default: 0)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of output lines to return (default: 100)"
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            execution_mode: ToolExecutionMode::Parallel,
            approval: ToolApprovalPolicy::NeverAsk,
            source: ToolSource::Builtin,
            prompt_snippet: Some("Search file contents for patterns".to_string()),
            prompt_guidelines: Vec::new(),
        }
    }

    fn execute(&self, ctx: ToolContext, input: Value, _updates: ToolUpdateSink) -> ToolFuture<'_> {
        Box::pin(async move {
            let input: GrepInput = serde_json::from_value(input).context("invalid grep input")?;
            validate_input(&input)?;
            let search_path = ctx.workspace.resolve_search_path(input.path.as_deref())?;
            let limit = input.limit.unwrap_or(DEFAULT_LIMIT).max(1);
            let output = run_grep_command(&ctx, &input, &search_path, limit).await?;
            Ok(build_result(output, limit))
        })
    }
}

struct GrepOutput {
    lines: Vec<String>,
    match_limit_reached: bool,
    lines_truncated: bool,
}

fn validate_input(input: &GrepInput) -> anyhow::Result<()> {
    if input.pattern.trim().is_empty() {
        bail!("grep pattern must not be empty");
    }
    Ok(())
}

async fn run_grep_command(
    ctx: &ToolContext,
    input: &GrepInput,
    search_path: &Path,
    limit: usize,
) -> anyhow::Result<GrepOutput> {
    if let Some(rg) = find_program("rg") {
        return run_rg(ctx, rg, input, search_path, limit).await;
    }
    if let Some(grep) = find_program("grep") {
        return run_system_grep(ctx, grep, input, search_path, limit).await;
    }
    bail!("neither rg nor grep is available")
}

async fn run_rg(
    ctx: &ToolContext,
    rg: PathBuf,
    input: &GrepInput,
    search_path: &Path,
    limit: usize,
) -> anyhow::Result<GrepOutput> {
    let mut args = vec![
        "--line-number".to_string(),
        "--color=never".to_string(),
        "--hidden".to_string(),
    ];
    if input.ignore_case.unwrap_or(false) {
        args.push("--ignore-case".to_string());
    }
    if input.literal.unwrap_or(false) {
        args.push("--fixed-strings".to_string());
    }
    if let Some(context) = input.context.filter(|context| *context > 0) {
        args.extend(["--context".to_string(), context.to_string()]);
    }
    if let Some(glob) = input.glob.as_ref() {
        args.extend(["--glob".to_string(), glob.clone()]);
    }
    args.extend([
        "--".to_string(),
        input.pattern.clone(),
        search_path.display().to_string(),
    ]);
    let output = run_capture(
        ProcessSpec {
            program: rg,
            args,
            cwd: ctx.cwd.clone(),
            timeout: None,
            merge_stderr: false,
        },
        ctx.cancellation.clone(),
    )
    .await?;
    parse_process_output("rg", output, limit)
}

async fn run_system_grep(
    ctx: &ToolContext,
    grep: PathBuf,
    input: &GrepInput,
    search_path: &Path,
    limit: usize,
) -> anyhow::Result<GrepOutput> {
    let is_dir = search_path.is_dir();
    let mut args = Vec::new();
    if is_dir {
        args.push("-R".to_string());
    }
    args.extend(["-n".to_string(), "-I".to_string()]);
    if input.ignore_case.unwrap_or(false) {
        args.push("-i".to_string());
    }
    if input.literal.unwrap_or(false) {
        args.push("-F".to_string());
    }
    if let Some(context) = input.context.filter(|context| *context > 0) {
        args.extend(["-C".to_string(), context.to_string()]);
    }
    if is_dir {
        args.extend([
            "--exclude-dir=.git".to_string(),
            "--exclude-dir=node_modules".to_string(),
        ]);
        if let Some(glob) = input.glob.as_ref() {
            args.push(format!("--include={glob}"));
        }
    }
    args.extend([
        "--".to_string(),
        input.pattern.clone(),
        search_path.display().to_string(),
    ]);
    let output = run_capture(
        ProcessSpec {
            program: grep,
            args,
            cwd: ctx.cwd.clone(),
            timeout: None,
            merge_stderr: false,
        },
        ctx.cancellation.clone(),
    )
    .await?;
    parse_process_output("grep", output, limit)
}

fn parse_process_output(
    program: &str,
    output: crate::tools::process::ProcessOutput,
    limit: usize,
) -> anyhow::Result<GrepOutput> {
    if output.exit_code == Some(1) && output.stdout.is_empty() {
        return Ok(GrepOutput {
            lines: Vec::new(),
            match_limit_reached: false,
            lines_truncated: false,
        });
    }
    if output.exit_code.is_some_and(|code| code != 0) && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "{program} failed: {}",
            if stderr.is_empty() {
                "unknown error"
            } else {
                &stderr
            }
        );
    }

    let mut lines = Vec::new();
    let mut match_limit_reached = false;
    let mut lines_truncated = false;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if lines.len() >= limit {
            match_limit_reached = true;
            break;
        }
        let (line, was_truncated) = truncate_line(line, GREP_MAX_LINE_CHARS);
        lines_truncated |= was_truncated;
        lines.push(line);
    }
    if lines.len() >= limit {
        match_limit_reached = true;
    }
    Ok(GrepOutput {
        lines,
        match_limit_reached,
        lines_truncated,
    })
}

fn build_result(output: GrepOutput, limit: usize) -> ToolResult {
    if output.lines.is_empty() {
        return text_result("No matches found".to_string(), json!({}));
    }

    let raw = output.lines.join("\n");
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

    if output.match_limit_reached {
        details.insert("matchLimitReached".to_string(), json!(limit));
        notices.push(format!("{limit} matches limit reached"));
    }
    if output.lines_truncated {
        details.insert("linesTruncated".to_string(), json!(true));
        notices.push("some lines truncated".to_string());
    }
    if truncated.report.truncated {
        details.insert("truncation".to_string(), json!(truncated.report));
        notices.push(format!(
            "{}KB limit reached",
            DEFAULT_MODEL_MAX_BYTES / 1024
        ));
    }
    if !notices.is_empty() {
        text.push_str(&format!("\n\n[Truncated: {}]", notices.join(", ")));
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
