//! Backups. We run the dump tool of each database: `mysqldump`, `pg_dump` and `sqlite3`.
//!
//! The tools know every detail of their own database, so the dump can be loaded again.

use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;

#[cfg(unix)]
use crate::constants::{HOMEBREW_OPT_DIR, TOOL_DIRS};
use crate::constants::{
    MSSQL_NO_BACKUP, MYSQL_PASSWORD_ENV, PARTIAL_DUMP_SUFFIX, POSTGRES_PASSWORD_ENV,
};
use crate::error::{AppError, AppResult};
use crate::model::{ConnectionConfig, Driver};

/// Writes SQL dumps with the dump tools that are installed on this computer.
pub struct DumpRepository;

impl DumpRepository {
    /// Writes a full SQL dump (structure and data) of one database to `path`.
    ///
    /// `config` must hold the host and port we really connect to. With an SSH
    /// tunnel that is the local end of the tunnel.
    pub async fn dump(config: &ConnectionConfig, path: &Path) -> AppResult<()> {
        if config.driver == Driver::Mssql {
            return Err(MSSQL_NO_BACKUP.into());
        }
        // The tool writes to a side file first. So a dump that fails leaves no
        // half file, and does not harm an older backup that is already at `path`.
        let mut partial = OsString::from(path);
        partial.push(PARTIAL_DUMP_SUFFIX);
        let partial = PathBuf::from(partial);

        let mut result = Self::run(config, &partial).await;
        if result.is_ok() {
            result = std::fs::rename(&partial, path).map_err(AppError::from);
        }
        if result.is_err() {
            // The side file may not exist. That is fine.
            let _ = std::fs::remove_file(&partial);
        }
        result
    }

    /// Runs the dump tool and waits for it. Its error text becomes our error.
    async fn run(config: &ConnectionConfig, output: &Path) -> AppResult<()> {
        let mut command = match config.driver {
            Driver::Mysql => Self::mysql(config, output).await?,
            Driver::Postgres => Self::postgres(config, output)?,
            Driver::Sqlite => Self::sqlite(config, output)?,
            Driver::Mssql => return Err(MSSQL_NO_BACKUP.into()),
        };
        super::hide_console(&mut command);
        command
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let finished = command.spawn()?.wait_with_output().await?;
        if finished.status.success() {
            return Ok(());
        }
        let reason = String::from_utf8_lossy(&finished.stderr);
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(AppError::Message(format!(
                "The dump tool failed ({})",
                finished.status
            )));
        }
        Err(AppError::Message(reason.to_owned()))
    }

    async fn mysql(config: &ConnectionConfig, output: &Path) -> AppResult<Command> {
        if config.database.is_empty() {
            return Err("Choose a database first".into());
        }
        let tool = Self::find(&["mysqldump", "mariadb-dump"]).ok_or(
            "mysqldump was not found. Install the MySQL or MariaDB client tools (macOS: brew install mysql-client)",
        )?;
        let mut command = Command::new(&tool);
        command
            // Without this, the host "localhost" means a socket file and not the port.
            .arg("--protocol=TCP")
            .arg(format!("--host={}", config.host))
            .arg(format!("--port={}", config.port))
            .arg("--single-transaction")
            .arg("--routines")
            .arg("--triggers")
            // Tablespaces need the PROCESS right, which most users do not have.
            .arg("--no-tablespaces")
            .arg(Self::option("--result-file=", output))
            .stdout(Stdio::null());
        if !config.user.is_empty() {
            command.arg(format!("--user={}", config.user));
        }
        if !config.password.is_empty() {
            // Not an argument: other users of the computer can read arguments.
            command.env(MYSQL_PASSWORD_ENV, &config.password);
        }
        // The mysqldump of MySQL 8 asks for column statistics. MariaDB and MySQL 5.7
        // servers do not have them, and the dump fails. Other clients do not know the flag.
        // ponytail: we never dump histogram statistics, also not from a MySQL 8
        // server. They are not data. Ask the server for its version if they are needed.
        if Self::knows(&tool, "column-statistics").await {
            command.arg("--column-statistics=0");
        }
        // "--" ends the options. A database name can then never be read as one.
        command.arg("--").arg(&config.database);
        Ok(command)
    }

    fn postgres(config: &ConnectionConfig, output: &Path) -> AppResult<Command> {
        let tool = Self::find(&["pg_dump"])
            .ok_or("pg_dump was not found. Install the PostgreSQL client tools (macOS: brew install libpq)")?;
        let mut command = Command::new(tool);
        command
            .arg(format!("--host={}", config.host))
            .arg(format!("--port={}", config.port))
            // Never wait for a typed password. Nobody can type it.
            .arg("--no-password")
            .arg(Self::option("--file=", output))
            .stdout(Stdio::null());
        if !config.user.is_empty() {
            command.arg(format!("--username={}", config.user));
        }
        if !config.database.is_empty() {
            command.arg(format!("--dbname={}", config.database));
        }
        if !config.password.is_empty() {
            command.env(POSTGRES_PASSWORD_ENV, &config.password);
        }
        Ok(command)
    }

    /// `SQLite` has no dump tool of its own. The `sqlite3` shell prints the dump.
    fn sqlite(config: &ConnectionConfig, output: &Path) -> AppResult<Command> {
        let tool = Self::find(&["sqlite3"]).ok_or(
            "sqlite3 was not found. Install the sqlite3 tool (macOS: brew install sqlite)",
        )?;
        let mut command = Command::new(tool);
        command
            .arg("-readonly")
            // Stop at the first error, so a broken file gives an error and not half a dump.
            .arg("-bail")
            .arg(&config.database)
            .arg(".dump")
            .stdout(File::create(output)?);
        Ok(command)
    }

    /// Builds one argument like `--file=/some/path`. The path does not have to be UTF-8.
    fn option(name: &str, path: &Path) -> OsString {
        let mut option = OsString::from(name);
        option.push(path);
        option
    }

    /// Does the help text of the tool name this option?
    async fn knows(tool: &Path, option: &str) -> bool {
        let mut command = Command::new(tool);
        super::hide_console(&mut command);
        let help = command.arg("--help").stdin(Stdio::null()).output().await;
        match help {
            Ok(help) => String::from_utf8_lossy(&help.stdout).contains(option),
            Err(_) => false,
        }
    }

    /// Finds the first of `names` that is installed. Earlier names win.
    fn find(names: &[&str]) -> Option<PathBuf> {
        let folders = Self::folders();
        for name in names {
            let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
            for folder in &folders {
                let path = folder.join(&file);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Where a tool can be: the folders of PATH first, then the usual install places.
    fn folders() -> Vec<PathBuf> {
        let mut folders = Vec::new();
        if let Some(path) = std::env::var_os("PATH") {
            folders.extend(std::env::split_paths(&path));
        }
        // These are macOS and Linux folders. On Windows "/usr/bin" would mean a folder
        // on the current drive that any user can create, so Windows uses PATH only.
        #[cfg(unix)]
        {
            for folder in TOOL_DIRS {
                folders.push(PathBuf::from(folder));
            }
            // ponytail: any Homebrew PostgreSQL version, in no fixed order. pg_dump
            // refuses a server that is newer than itself; pick the newest if that hurts.
            if let Ok(entries) = std::fs::read_dir(HOMEBREW_OPT_DIR) {
                for entry in entries.flatten() {
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("postgresql@")
                    {
                        folders.push(entry.path().join("bin"));
                    }
                }
            }
        }
        folders
    }
}
