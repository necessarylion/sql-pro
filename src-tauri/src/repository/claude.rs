//! Talks to the Claude Code CLI (`claude`) that is installed on this computer.
//!
//! It uses the user's own Claude login, so the app needs no API key.
//! All built-in tools of Claude are off. Claude only writes text; the app runs the SQL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use futures_util::future::{AbortHandle, Abortable};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[cfg(not(windows))]
use crate::constants::{CLAUDE_HOME_PATHS, TOOL_DIRS};
#[cfg(windows)]
use crate::constants::{CLAUDE_NEEDS_NATIVE, CLAUDE_NPM_SCRIPT, CLAUDE_WINDOWS_PATHS};
use crate::constants::{
    CLAUDE_NOT_FOUND, CLAUDE_REPLY_SCHEMA, CLAUDE_SESSION_GONE, CLAUDE_STOPPED,
    CLAUDE_SYSTEM_PROMPT, CLAUDE_TIMEOUT, CLAUDE_TIMEOUT_SECONDS,
};
use crate::error::{AppError, AppResult};
use crate::model::{AiOptions, Attachment};

/// What one run of `claude -p --output-format json` prints. We read only these fields.
#[derive(Deserialize)]
pub struct ClaudeOutput {
    pub session_id: String,
    #[serde(default)]
    pub is_error: bool,
    /// The reply as text.
    #[serde(default)]
    pub result: String,
    /// The reply as JSON, when it matched `--json-schema`.
    #[serde(default)]
    pub structured_output: Option<serde_json::Value>,
    /// Why a run failed, when `result` is empty. Like "No conversation found …".
    #[serde(default)]
    pub errors: Vec<String>,
}

/// How to start `claude`: the program, and the arguments that go before ours.
/// Mostly just `claude`. On Windows with only the npm package it is `node cli.js`.
#[derive(Clone)]
struct Launcher {
    program: PathBuf,
    first_args: Vec<PathBuf>,
}

impl Launcher {
    fn direct(program: PathBuf) -> Self {
        Self {
            program,
            first_args: Vec::new(),
        }
    }
}

/// Waits for `run`, at most `limit`. When the time is up, `run` is dropped,
/// and a child process inside it is killed (`kill_on_drop`), the same as a stop.
pub(crate) async fn within<T>(
    limit: Duration,
    run: impl Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::time::timeout(limit, run)
        .await
        .map_err(|_| AppError::from(CLAUDE_TIMEOUT))?
}

/// Runs `claude`. It finds the program once and keeps it.
#[derive(Default)]
pub struct ClaudeCli {
    launcher: Mutex<Option<Launcher>>,
    /// The run of each tab, so the user can stop it.
    runs: Mutex<HashMap<String, AbortHandle>>,
    /// The folder `claude` runs in. See `CLAUDE_FOLDER`. The temp folder until it is set.
    folder: Mutex<Option<PathBuf>>,
}

impl ClaudeCli {
    /// Sends `prompt` to Claude and waits for the reply.
    /// `session` continues an earlier conversation. `key` names the run for `stop`.
    /// The model and effort in `options` must be checked before.
    pub async fn ask(
        &self,
        key: &str,
        prompt: &str,
        images: &[Attachment],
        session: Option<&str>,
        options: &AiOptions,
    ) -> AppResult<ClaudeOutput> {
        let launcher = self.find().await?;
        let message = Self::user_message(prompt, images);
        let folder = self
            .folder
            .lock()
            .clone()
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&folder)?;
        let (handle, registration) = AbortHandle::new_pair();
        if let Some(older) = self.runs.lock().insert(key.to_owned(), handle) {
            older.abort();
        }
        let limit = Duration::from_secs(CLAUDE_TIMEOUT_SECONDS);
        let run = Self::run(&launcher, &folder, &message, session, options);
        let run = Abortable::new(within(limit, run), registration).await;
        self.runs.lock().remove(key);
        // A stop drops the run. The child process is killed on drop.
        let output = run.map_err(|_| AppError::from(CLAUDE_STOPPED))??;
        if output.is_error {
            let reason = if output.result.trim().is_empty() {
                output.errors.join(" ")
            } else {
                output.result
            };
            if session.is_some() && reason.contains("No conversation found") {
                return Err(CLAUDE_SESSION_GONE.into());
            }
            return Err(AppError::Message(format!("Claude Code: {reason}")));
        }
        Ok(output)
    }

    /// Sets the folder `claude` runs in. It must stay the same between app starts.
    pub fn set_folder(&self, folder: PathBuf) {
        *self.folder.lock() = Some(folder);
    }

    /// Stops the run of `key`. Does nothing when none runs.
    pub fn stop(&self, key: &str) {
        if let Some(handle) = self.runs.lock().remove(key) {
            handle.abort();
        }
    }

    /// One user message as a line of `stream-json`: the text, then each image as a
    /// base64 block. So Claude sees the images without its Read tool.
    fn user_message(prompt: &str, images: &[Attachment]) -> String {
        let mut content = vec![serde_json::json!({ "type": "text", "text": prompt })];
        for image in images {
            content.push(serde_json::json!({
                "type": "image",
                "source": { "type": "base64", "media_type": image.media_type, "data": image.data },
            }));
        }
        let line = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": content },
        });
        format!("{line}\n")
    }

    async fn run(
        launcher: &Launcher,
        folder: &Path,
        message: &str,
        session: Option<&str>,
        options: &AiOptions,
    ) -> AppResult<ClaudeOutput> {
        let mut command = Command::new(&launcher.program);
        command.args(&launcher.first_args);
        command.args([
            "-p",
            // Images need `stream-json` input, and that needs `stream-json` output.
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            // No built-in tools, no MCP servers, no skills, no user settings or hooks.
            "--tools",
            "",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--setting-sources",
            "",
            "--system-prompt",
            CLAUDE_SYSTEM_PROMPT,
            "--json-schema",
            CLAUDE_REPLY_SCHEMA,
        ]);
        if let Some(session) = session {
            command.args(["--resume", session]);
        }
        if let Some(model) = &options.model {
            command.args(["--model", model]);
        }
        if let Some(effort) = &options.effort {
            command.args(["--effort", effort]);
        }
        // `--resume` finds a session by its folder, so always run in the same one.
        super::hide_console(&mut command);
        command
            .current_dir(folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        // The message goes on stdin. It can hold data and be long, so not on the command line.
        // It is written while the output is read, so a full pipe cannot block both sides.
        let stdin = child.stdin.take();
        let write = async move {
            if let Some(mut stdin) = stdin {
                stdin.write_all(message.as_bytes()).await?;
            }
            std::io::Result::Ok(())
        };
        let (written, finished) = futures_util::future::join(write, child.wait_with_output()).await;
        let finished = finished?;
        // The last `result` line holds the answer. The lines before are progress.
        let result = String::from_utf8_lossy(&finished.stdout)
            .lines()
            .rev()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|value| value.get("type").and_then(serde_json::Value::as_str) == Some("result"));
        if let Some(value) = result
            && let Ok(output) = serde_json::from_value::<ClaudeOutput>(value)
        {
            return Ok(output);
        }
        written?;
        let stderr = String::from_utf8_lossy(&finished.stderr);
        let stdout = String::from_utf8_lossy(&finished.stdout);
        if session.is_some() && format!("{stderr}{stdout}").contains("No conversation found") {
            return Err(CLAUDE_SESSION_GONE.into());
        }
        let reason = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        Err(AppError::Message(format!(
            "Claude Code failed ({}): {reason}",
            finished.status
        )))
    }

    /// How to start `claude`. Found once, then kept.
    async fn find(&self) -> AppResult<Launcher> {
        if let Some(launcher) = self.launcher.lock().clone() {
            return Ok(launcher);
        }
        let launcher = Self::search().await?;
        *self.launcher.lock() = Some(launcher.clone());
        Ok(launcher)
    }

    /// A macOS app started from the Finder has a very short PATH. So we look in the
    /// home folder first: the native install updates itself there. An old copy from
    /// Homebrew or npm can come first in the PATH and not know newer flags.
    /// Then we ask the login shell, then look in the usual folders.
    #[cfg(not(windows))]
    async fn search() -> AppResult<Launcher> {
        let mut found = None;
        if let Some(home) = std::env::var_os("HOME") {
            found = CLAUDE_HOME_PATHS
                .iter()
                .map(|relative| Path::new(&home).join(relative))
                .find(|path| path.is_file());
        }
        if found.is_none() {
            found = Self::from_shell().await;
        }
        if found.is_none() {
            found = TOOL_DIRS
                .iter()
                .map(|dir| Path::new(dir).join("claude"))
                .find(|path| path.is_file());
        }
        found
            .map(Launcher::direct)
            .ok_or_else(|| AppError::from(CLAUDE_NOT_FOUND))
    }

    /// Asks `$SHELL -lc 'command -v claude'`.
    #[cfg(not(windows))]
    async fn from_shell() -> Option<PathBuf> {
        let shell = std::env::var_os("SHELL")?;
        let output = Command::new(shell)
            .args(["-lc", "command -v claude"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        // A login shell may print other lines first. The path is the last one.
        let path = PathBuf::from(text.lines().last()?.trim());
        (output.status.success() && path.is_file()).then_some(path)
    }

    /// `where claude` first, then the usual install folders. A real `claude.exe` wins.
    ///
    /// Rust has special rules for running `.cmd` and `.bat` files with arguments
    /// (CVE-2024-24576), and our system prompt is a long argument. So the npm
    /// `claude.cmd` shim is never run. We run `node` with the package's `cli.js` instead.
    #[cfg(windows)]
    async fn search() -> AppResult<Launcher> {
        let mut candidates = Self::where_all("claude").await;
        for (variable, relative) in CLAUDE_WINDOWS_PATHS {
            if let Some(folder) = std::env::var_os(variable) {
                candidates.push(Path::new(&folder).join(relative));
            }
        }
        candidates.retain(|path| path.is_file());
        if let Some(exe) = candidates
            .iter()
            .find(|path| Self::has_extension(path, "exe"))
        {
            return Ok(Launcher::direct(exe.clone()));
        }
        if candidates.is_empty() {
            return Err(CLAUDE_NOT_FOUND.into());
        }
        for shim in &candidates {
            let Some(folder) = shim.parent() else {
                continue;
            };
            let script = folder.join(CLAUDE_NPM_SCRIPT);
            if !script.is_file() {
                continue;
            }
            let mut nodes = vec![folder.join("node.exe")];
            nodes.extend(Self::where_all("node").await);
            if let Some(node) = nodes
                .into_iter()
                .find(|path| path.is_file() && Self::has_extension(path, "exe"))
            {
                return Ok(Launcher {
                    program: node,
                    first_args: vec![script],
                });
            }
        }
        Err(CLAUDE_NEEDS_NATIVE.into())
    }

    /// Every path that `where name` prints. No console window opens.
    #[cfg(windows)]
    async fn where_all(name: &str) -> Vec<PathBuf> {
        let mut command = Command::new("where");
        command
            .arg(name)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        super::hide_console(&mut command);
        let Ok(output) = command.output().await else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| PathBuf::from(line.trim()))
            .filter(|path| !path.as_os_str().is_empty())
            .collect()
    }

    #[cfg(windows)]
    fn has_extension(path: &Path, extension: &str) -> bool {
        path.extension()
            .is_some_and(|found| found.eq_ignore_ascii_case(extension))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::within;
    use crate::constants::CLAUDE_TIMEOUT;
    use crate::error::AppResult;

    #[tokio::test]
    async fn a_slow_run_ends_with_the_timeout_error() {
        let slow = async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            AppResult::Ok(())
        };
        let error = within(Duration::from_millis(50), slow)
            .await
            .expect_err("timeout");
        assert_eq!(error.to_string(), CLAUDE_TIMEOUT);

        let fast = async { AppResult::Ok(7) };
        assert_eq!(within(Duration::from_secs(5), fast).await.expect("fast"), 7);
    }

    /// The timeout drops the run, and that kills its child process, like a stop.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_timeout_kills_the_child_process() {
        let mut child = tokio::process::Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .expect("sleep");
        let pid = child.id().expect("pid").to_string();
        let run = async move {
            child.wait().await?;
            AppResult::Ok(())
        };
        assert!(within(Duration::from_millis(50), run).await.is_err());
        // A killed process is gone, or a zombie until it is reaped. It never sleeps on.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let state = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid])
            .output()
            .expect("ps");
        let state = String::from_utf8_lossy(&state.stdout);
        assert!(
            state.trim().is_empty() || state.trim().starts_with('Z'),
            "{state}"
        );
    }
}
