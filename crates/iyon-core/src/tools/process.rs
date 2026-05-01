use std::{path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Option<Duration>,
    pub merge_stderr: bool,
}

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
}

pub fn find_program(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

pub async fn run_capture(
    spec: ProcessSpec,
    cancellation: CancellationToken,
) -> anyhow::Result<ProcessOutput> {
    if cancellation.is_cancelled() {
        bail!("process cancelled before start");
    }

    let mut child = command_for_spec(&spec)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn process: {}", spec.program.display()))?;

    let stdout = child
        .stdout
        .take()
        .context("failed to capture process stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("failed to capture process stderr")?;
    let output_fut = collect_child_output(child, stdout, stderr, spec.merge_stderr);

    if let Some(timeout) = spec.timeout {
        tokio::select! {
            result = output_fut => result,
            () = cancellation.cancelled() => bail!("process cancelled"),
            () = tokio::time::sleep(timeout) => bail!("process timed out after {}s", timeout.as_secs()),
        }
    } else {
        tokio::select! {
            result = output_fut => result,
            () = cancellation.cancelled() => bail!("process cancelled"),
        }
    }
}

fn command_for_spec(spec: &ProcessSpec) -> Command {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .kill_on_drop(true);
    command
}

async fn collect_child_output(
    mut child: tokio::process::Child,
    stdout: impl AsyncRead + Unpin + Send + 'static,
    stderr: impl AsyncRead + Unpin + Send + 'static,
    merge_stderr: bool,
) -> anyhow::Result<ProcessOutput> {
    let stdout_task = tokio::spawn(read_all(stdout));
    let stderr_task = tokio::spawn(read_all(stderr));
    let status = child.wait().await.context("failed to wait for process")?;
    let mut stdout = stdout_task.await.context("stdout reader task failed")??;
    let stderr = stderr_task.await.context("stderr reader task failed")??;
    let stderr = if merge_stderr {
        stdout.extend_from_slice(&stderr);
        Vec::new()
    } else {
        stderr
    };

    Ok(ProcessOutput {
        stdout,
        stderr,
        exit_code: status.code(),
    })
}

async fn read_all(mut reader: impl AsyncRead + Unpin + Send + 'static) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.read_to_end(&mut output).await?;
    Ok(output)
}
