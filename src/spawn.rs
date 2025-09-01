use std::process::Stdio;

use tokio::{
    io::{self, AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
    task::JoinHandle,
    time::{timeout, Duration},
};

/// A task that reads lines from a source (stdin or command output) and sends them through an mpsc channel.
struct InputTask {
    pub handle: JoinHandle<anyhow::Result<()>>,
    // None if the task reads from stdin
    pub child: Option<Child>,
}

/// Spawn a task to read from stdin and send lines to the provided mpsc sender.
pub fn spawn_stdin_sender(
    tx: mpsc::Sender<String>,
    retrieval_timeout: Duration,
) -> anyhow::Result<InputTask> {
    let mut reader = BufReader::new(io::stdin()).lines();

    Ok(InputTask {
        handle: tokio::spawn(async move {
            loop {
                // Set a timeout to ensure non-blocking behavior,
                // especially responsive to user inputs like ctrl+c.
                // Continuously retry until cancellation to prevent loss of logs.
                let ret = timeout(retrieval_timeout, reader.next_line()).await;
                if ret.is_err() {
                    continue;
                }

                let ret = ret?;

                match ret {
                    Ok(Some(line)) => {
                        let escaped =
                            strip_ansi_escapes::strip_str(line.replace(['\n', '\t'], " "));
                        tx.send(escaped).await?;
                    }
                    _ => break,
                }
            }
            Ok(())
        }),
        child: None,
    })
}

/// Spawn a command and read its stdout and stderr, sending lines to the provided mpsc sender.
pub async fn spawn_cmd_result_sender(
    cmdstr: &str,
    tx: mpsc::Sender<String>,
    retrieval_timeout: Duration,
) -> anyhow::Result<InputTask> {
    let args: Vec<&str> = cmdstr.split_whitespace().collect();
    let mut child = Command::new(args[0])
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("stdout is not available"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("stderr is not available"))?;
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    Ok(InputTask {
        handle: tokio::spawn(async move {
            loop {
                tokio::select! {
                    stdout_res = timeout(retrieval_timeout, stdout_reader.next_line()) => {
                        match stdout_res {
                            Ok(Ok(Some(line))) => {
                                let escaped = strip_ansi_escapes::strip_str(line.replace(['\n', '\t'], " "));
                                tx.send(escaped).await?;
                            },
                            _ => break,
                        }
                    },
                    stderr_res = timeout(retrieval_timeout, stderr_reader.next_line()) => {
                        match stderr_res {
                            Ok(Ok(Some(line))) => {
                                let escaped = strip_ansi_escapes::strip_str(line.replace(['\n', '\t'], " "));
                                tx.send(escaped).await?;
                            },
                            _ => break,
                        }
                    }
                }
            }
            Ok(())
        }),
        child: Some(child),
    })
}
