use std::{
    collections::VecDeque,
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use iyon_api::ContentBlock;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::mpsc,
};

use crate::tools::{
    ToolApprovalPolicy, ToolContext, ToolDefinition, ToolExecutionMode, ToolExecutor, ToolFuture,
    ToolResult, ToolSource, ToolUpdate, ToolUpdateSink,
    output::{DEFAULT_MODEL_MAX_BYTES, DEFAULT_MODEL_MAX_LINES, ModelOutputLimits, truncate_tail},
};

const ROLLING_BUFFER_BYTES: usize = DEFAULT_MODEL_MAX_BYTES * 2;

#[derive(Debug, Default)]
pub struct BashTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BashInput {
    command: String,
    timeout: Option<u64>,
}

struct BashRunOutput {
    output: String,
    exit_code: Option<i32>,
    full_output_path: Option<PathBuf>,
}

#[derive(Default)]
struct RollingOutput {
    chunks: VecDeque<Vec<u8>>,
    chunks_bytes: usize,
    total_bytes: usize,
    full_output_path: Option<PathBuf>,
    full_output_file: Option<std::fs::File>,
}

impl ToolExecutor for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".to_string(),
            label: "bash".to_string(),
            description: format!(
                "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last {DEFAULT_MODEL_MAX_LINES} lines or {}KB (whichever is hit first). If truncated, full output is saved to a temp file. Optionally provide a timeout in seconds.",
                DEFAULT_MODEL_MAX_BYTES / 1024
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Bash command to execute"
                    },
                    "timeout": {
                        "type": "number",
                        "description": "Timeout in seconds (optional, no default timeout)"
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            execution_mode: ToolExecutionMode::Sequential,
            approval: ToolApprovalPolicy::NeverAsk,
            source: ToolSource::Builtin,
            prompt_snippet: Some("Execute bash commands (ls, grep, find, etc.)".to_string()),
            prompt_guidelines: Vec::new(),
        }
    }

    fn execute(&self, ctx: ToolContext, input: Value, updates: ToolUpdateSink) -> ToolFuture<'_> {
        Box::pin(async move {
            let input: BashInput = serde_json::from_value(input).context("invalid bash input")?;
            validate_input(&input)?;
            ensure_not_cancelled(&ctx)?;
            let output = run_bash(&ctx, &input, updates).await?;
            Ok(build_result(output))
        })
    }
}

fn validate_input(input: &BashInput) -> anyhow::Result<()> {
    if input.command.trim().is_empty() {
        bail!("bash command must not be empty");
    }
    Ok(())
}

async fn run_bash(
    ctx: &ToolContext,
    input: &BashInput,
    updates: ToolUpdateSink,
) -> anyhow::Result<BashRunOutput> {
    let shell = resolve_shell();
    let mut child = Command::new(shell)
        .arg("-lc")
        .arg(&input.command)
        .current_dir(&ctx.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("failed to spawn bash command")?;

    let stdout = child.stdout.take().context("failed to capture stdout")?;
    let stderr = child.stderr.take().context("failed to capture stderr")?;
    let (chunk_tx, mut chunk_rx) = mpsc::channel(32);
    tokio::spawn(read_stream(stdout, chunk_tx.clone()));
    tokio::spawn(read_stream(stderr, chunk_tx));

    let mut output = RollingOutput::default();
    let mut timeout_sleep = input
        .timeout
        .filter(|timeout| *timeout > 0)
        .map(|timeout| Box::pin(tokio::time::sleep(Duration::from_secs(timeout))));

    loop {
        tokio::select! {
            chunk = chunk_rx.recv() => {
                if let Some(chunk) = chunk {
                    output.push_chunk(chunk)?;
                    maybe_send_update(&updates, &output).await;
                    continue;
                }
            }
            status = child.wait() => {
                let status = status.context("failed to wait for bash command")?;
                drain_remaining_chunks(&mut chunk_rx, &mut output)?;
                return Ok(BashRunOutput {
                    output: output.rolling_text(),
                    exit_code: status.code(),
                    full_output_path: output.full_output_path,
                });
            }
            () = ctx.cancellation.cancelled() => {
                let _ = child.kill().await;
                bail!("bash command cancelled");
            }
            () = async {
                if let Some(sleep) = timeout_sleep.as_mut() {
                    sleep.as_mut().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                let _ = child.kill().await;
                bail!("bash command timed out after {}s", input.timeout.unwrap_or_default());
            }
        }
    }
}

async fn read_stream(mut reader: impl AsyncRead + Unpin, tx: mpsc::Sender<Vec<u8>>) {
    let mut buffer = vec![0; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if tx.send(buffer[..n].to_vec()).await.is_err() {
                    return;
                }
            }
        }
    }
}

fn drain_remaining_chunks(
    chunk_rx: &mut mpsc::Receiver<Vec<u8>>,
    output: &mut RollingOutput,
) -> anyhow::Result<()> {
    while let Ok(chunk) = chunk_rx.try_recv() {
        output.push_chunk(chunk)?;
    }
    Ok(())
}

async fn maybe_send_update(updates: &ToolUpdateSink, output: &RollingOutput) {
    let text = output.rolling_text();
    if text.is_empty() {
        return;
    }
    let truncated = truncate_tail(
        &text,
        ModelOutputLimits {
            max_lines: 20,
            max_bytes: 8 * 1024,
        },
    );
    let _ = updates.send(ToolUpdate::Text(truncated.text)).await;
}

fn build_result(output: BashRunOutput) -> ToolResult {
    let truncated = truncate_tail(
        &output.output,
        ModelOutputLimits {
            max_lines: DEFAULT_MODEL_MAX_LINES,
            max_bytes: DEFAULT_MODEL_MAX_BYTES,
        },
    );
    let mut details = serde_json::Map::new();
    details.insert("exitCode".to_string(), json!(output.exit_code));
    if truncated.report.truncated {
        details.insert("truncation".to_string(), json!(truncated.report));
    }
    if let Some(path) = output.full_output_path {
        details.insert(
            "fullOutputPath".to_string(),
            json!(path.display().to_string()),
        );
    }

    let mut text = truncated.text;
    if let Some(code) = output.exit_code
        && code != 0
    {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!("[Command exited with code {code}]"));
    }

    ToolResult {
        content: vec![ContentBlock::Text { text }],
        details: Value::Object(details),
        is_error: output.exit_code.is_some_and(|code| code != 0),
        terminate: false,
    }
}

fn resolve_shell() -> String {
    if std::path::Path::new("/bin/bash").exists() {
        return "/bin/bash".to_string();
    }
    if let Some(path) = crate::tools::process::find_program("bash") {
        return path.display().to_string();
    }
    "/bin/sh".to_string()
}

fn ensure_not_cancelled(ctx: &ToolContext) -> anyhow::Result<()> {
    if ctx.cancellation.is_cancelled() {
        bail!("bash tool cancelled");
    }
    Ok(())
}

impl RollingOutput {
    fn push_chunk(&mut self, chunk: Vec<u8>) -> anyhow::Result<()> {
        self.total_bytes = self.total_bytes.saturating_add(chunk.len());
        self.ensure_full_output_file_if_needed()?;
        if let Some(file) = self.full_output_file.as_mut() {
            file.write_all(&chunk)
                .context("failed to write bash output log")?;
        }

        self.chunks_bytes = self.chunks_bytes.saturating_add(chunk.len());
        self.chunks.push_back(chunk);
        while self.chunks_bytes > ROLLING_BUFFER_BYTES && self.chunks.len() > 1 {
            if let Some(removed) = self.chunks.pop_front() {
                self.chunks_bytes = self.chunks_bytes.saturating_sub(removed.len());
            }
        }
        Ok(())
    }

    fn ensure_full_output_file_if_needed(&mut self) -> anyhow::Result<()> {
        if self.total_bytes <= DEFAULT_MODEL_MAX_BYTES || self.full_output_file.is_some() {
            return Ok(());
        }
        let path = temp_output_path();
        let mut file = std::fs::File::create(&path)
            .with_context(|| format!("failed to create bash output log: {}", path.display()))?;
        for chunk in &self.chunks {
            file.write_all(chunk)
                .context("failed to write bash output log")?;
        }
        self.full_output_path = Some(path);
        self.full_output_file = Some(file);
        Ok(())
    }

    fn rolling_text(&self) -> String {
        let bytes: Vec<u8> = self.chunks.iter().flatten().copied().collect();
        String::from_utf8_lossy(&bytes).to_string()
    }
}

fn temp_output_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("iyon-bash-{nanos}.log"))
}
