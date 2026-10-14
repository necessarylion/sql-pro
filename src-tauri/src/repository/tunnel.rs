//! SSH tunnels. We run the system `ssh` program, like Sequel Pro does.
//!
//! This gives us the user's SSH config, agent, known hosts and all key types for free.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt as _;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{Instant, sleep};

use crate::constants::{LOCALHOST, SSH_PASSWORD_ENV, SSH_TIMEOUT_SECONDS};
use crate::error::{AppError, AppResult};
use crate::model::SshConfig;

/// A running `ssh` process that forwards a local port to the database server.
/// The process is killed when this value is dropped.
pub struct SshTunnel {
    _child: Child,
    local_port: u16,
}

impl SshTunnel {
    /// Starts the tunnel and waits until the local port accepts connections.
    pub async fn open(ssh: &SshConfig, target_host: &str, target_port: u16) -> AppResult<Self> {
        // A host that starts with "-" would be read by ssh as an option.
        if ssh.host.is_empty() || ssh.host.starts_with('-') {
            return Err("The SSH host is not valid".into());
        }
        let local_port = Self::free_port()?;

        let mut command = Command::new("ssh");
        super::hide_console(&mut command);
        command
            .arg("-N")
            .arg("-L")
            .arg(format!(
                "{LOCALHOST}:{local_port}:{target_host}:{target_port}"
            ))
            .arg("-p")
            .arg(ssh.port.to_string())
            .args(["-o", "ExitOnForwardFailure=yes"])
            // A new host is trusted on first use. A host whose key changed is refused.
            .args(["-o", "StrictHostKeyChecking=accept-new"])
            .args(["-o", "ServerAliveInterval=30"])
            .args(["-o", "Compression=yes"])
            .args(["-o", "ConnectTimeout=10"])
            .args(["-o", "LogLevel=ERROR"]);
        if !ssh.user.is_empty() {
            command.arg("-l").arg(&ssh.user);
        }
        if !ssh.key_path.is_empty() {
            command.arg("-i").arg(&ssh.key_path);
        }
        if ssh.password.is_empty() {
            // Keys and the agent only. Never wait for typed input.
            command.args(["-o", "BatchMode=yes"]);
        } else {
            // ssh asks the helper script for the password (or the key passphrase).
            // The secret travels in the environment of the ssh process only.
            command
                .env("SSH_ASKPASS", Self::askpass()?)
                .env("SSH_ASKPASS_REQUIRE", "force")
                .env(SSH_PASSWORD_ENV, &ssh.password);
        }
        command
            .arg(&ssh.host)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command.spawn()?;
        let deadline = Instant::now() + Duration::from_secs(SSH_TIMEOUT_SECONDS);
        loop {
            if let Some(status) = child.try_wait()? {
                let mut reason = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    stderr.read_to_string(&mut reason).await?;
                }
                let reason = reason.trim();
                return Err(AppError::Message(format!(
                    "SSH tunnel failed ({status}): {reason}"
                )));
            }
            // ssh opens the local port only after login worked.
            if TcpStream::connect((LOCALHOST, local_port)).await.is_ok() {
                return Ok(Self {
                    _child: child,
                    local_port,
                });
            }
            if Instant::now() >= deadline {
                return Err("SSH tunnel timed out".into());
            }
            sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Asks the OS for a port that is free right now.
    fn free_port() -> AppResult<u16> {
        Ok(TcpListener::bind((LOCALHOST, 0))?.local_addr()?.port())
    }

    /// Writes the helper script that prints the password. The script holds no secret.
    #[cfg(unix)]
    fn askpass() -> AppResult<PathBuf> {
        use std::os::unix::fs::PermissionsExt as _;

        let name = format!("sqlpro-askpass-{}.sh", std::process::id());
        let path = std::env::temp_dir().join(name);
        std::fs::write(
            &path,
            format!("#!/bin/sh\nprintf '%s\\n' \"${SSH_PASSWORD_ENV}\"\n"),
        )?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        Ok(path)
    }

    // ponytail: the helper is a shell script, so SSH passwords work on macOS and
    // Linux only. Windows users can use a key or the agent.
    #[cfg(not(unix))]
    fn askpass() -> AppResult<PathBuf> {
        Err("SSH passwords are not supported on this system. Use a key or the SSH agent.".into())
    }
}
