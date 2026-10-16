//! Talks to the Claude Code CLI (`claude`) that is installed on this computer.
//!
//! It uses the user's own Claude login, so the app needs no API key.
//! All built-in tools of Claude are off. Claude only writes text; the app runs the SQL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use futures_util::future::{AbortHandle, Abortable};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::constants::{
    CLAUDE_HOME_PATHS, CLAUDE_NOT_FOUND, CLAUDE_REPLY_SCHEMA, CLAUDE_SESSION_GONE, CLAUDE_STOPPED,
    CLAUDE_SYSTEM_PROMPT, TOOL_DIRS,
};
use crate::error::{AppError, AppResult};
use crate::model::AiOptions;

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
}

/// Runs `claude`. It finds the program once and keeps the path.
#[derive(Default)]
pub struct ClaudeCli {
    path: Mutex<Option<PathBuf>>,
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
        session: Option<&str>,
        options: &AiOptions,
    ) -> AppResult<ClaudeOutput> {
        let path = self.find().await?;
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
        let run = Abortable::new(
            Self::run(&path, &folder, prompt, session, options),
            registration,
        )
        .await;
        self.runs.lock().remove(key);
        // A stop drops the run. The child process is killed on drop.
        let output = run.map_err(|_| AppError::from(CLAUDE_STOPPED))??;
        if output.is_error {
            return Err(AppError::Message(format!("Claude Code: {}", output.result)));
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

    async fn run(
        path: &Path,
        folder: &Path,
        prompt: &str,
        session: Option<&str>,
        options: &AiOptions,
    ) -> AppResult<ClaudeOutput> {
        let mut command = Command::new(path);
        command.args([
            "-p",
            "--output-format",
            "json",
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
        // The prompt goes on stdin. It can hold data and be long, so not on the command line.
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await?;
        }
        let finished = child.wait_with_output().await?;
        if let Ok(output) = serde_json::from_slice::<ClaudeOutput>(&finished.stdout) {
            return Ok(output);
        }
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

    /// The path of `claude`. A macOS app started from the Finder has a very short PATH,
    /// so we look in the home folder, then ask the login shell, then look in the usual folders.
    async fn find(&self) -> AppResult<PathBuf> {
        if let Some(path) = self.path.lock().clone() {
            return Ok(path);
        }
        // The home folder first: the native install updates itself there. An old copy
        // from Homebrew or npm can come first in the PATH and not know newer flags.
        let mut found = Self::home_paths().into_iter().find(|path| path.is_file());
        if found.is_none() {
            found = Self::from_shell().await;
        }
        if found.is_none() {
            found = TOOL_DIRS
                .iter()
                .map(|dir| Path::new(dir).join("claude"))
                .find(|path| path.is_file());
        }
        let path = found.ok_or_else(|| AppError::from(CLAUDE_NOT_FOUND))?;
        *self.path.lock() = Some(path.clone());
        Ok(path)
    }

    /// Asks `$SHELL -lc 'command -v claude'`. Unix only.
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

    fn home_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home) = home {
            for relative in CLAUDE_HOME_PATHS {
                paths.push(Path::new(&home).join(relative));
            }
        }
        paths
    }
}
