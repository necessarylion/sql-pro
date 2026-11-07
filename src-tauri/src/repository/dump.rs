//! Backups. We run the dump tool of each database: `mysqldump`, `pg_dump` and `sqlite3`.
//!
//! The tools know every detail of their own database, so the dump can be loaded again.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use futures_util::future::{AbortRegistration, Abortable};
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::Command;

use crate::constants::{
    BACKUP_CANCELLED, DUMP_PROGRESS_EVERY, MSSQL_NO_BACKUP, MYSQL_PASSWORD_ENV,
    PARTIAL_DUMP_SUFFIX, POSTGRES_PASSWORD_ENV,
};
#[cfg(unix)]
use crate::constants::{HOMEBREW_OPT_DIR, TOOL_DIRS};
use crate::error::{AppError, AppResult};
use crate::model::{ConnectionConfig, Driver, DumpProgress};

/// Writes SQL dumps with the dump tools that are installed on this computer.
pub struct DumpRepository;

impl DumpRepository {
    /// Writes a full SQL dump (structure and data) of one database to `path`.
    ///
    /// `config` must hold the host and port we really connect to. With an SSH
    /// tunnel that is the local end of the tunnel.
    ///
    /// `progress` gets how far it is, about once per second. An abort of `cancel` kills
    /// the tool. The dump then fails with `BACKUP_CANCELLED` and leaves no file.
    pub async fn dump(
        config: &ConnectionConfig,
        path: &Path,
        progress: impl FnMut(DumpProgress),
        cancel: AbortRegistration,
    ) -> AppResult<()> {
        if config.driver == Driver::Mssql {
            return Err(MSSQL_NO_BACKUP.into());
        }
        // The tool writes to a side file first. So a dump that fails leaves no
        // half file, and does not harm an older backup that is already at `path`.
        let partial = Self::partial_path(path);

        let mut result = Self::run(config, &partial, progress, cancel).await;
        if result.is_ok() {
            result = std::fs::rename(&partial, path).map_err(AppError::from);
        }
        if result.is_err() {
            // The side file may not exist. That is fine.
            let _ = std::fs::remove_file(&partial);
        }
        result
    }

    /// The side file of a dump to `path`, like `shop.sql.4242-7.part`. The name is new for
    /// each dump. So two tabs that back up to the same file never write into one side file.
    fn partial_path(path: &Path) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut partial = OsString::from(path);
        partial.push(format!(
            ".{}-{}{PARTIAL_DUMP_SUFFIX}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        PathBuf::from(partial)
    }

    /// Runs the dump tool and waits for it. Its error text becomes our error.
    async fn run(
        config: &ConnectionConfig,
        output: &Path,
        mut progress: impl FnMut(DumpProgress),
        cancel: AbortRegistration,
    ) -> AppResult<()> {
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
        let mut child = command.spawn()?;
        let stderr = child
            .stderr
            .take()
            .ok_or("The dump tool has no error output")?;
        // Not `lines()`: a line that is not UTF-8 would end the read with an error.
        let mut lines = BufReader::new(stderr).split(b'\n');
        let mut log = DumpLog::default();
        let started = Instant::now();
        let mut tick = tokio::time::interval(DUMP_PROGRESS_EVERY);
        let mut cancelled = Abortable::new(std::future::pending::<()>(), cancel);
        // The tool closes its error output when it ends.
        loop {
            tokio::select! {
                // A cancel wins over everything else.
                biased;
                _ = &mut cancelled => {
                    // Wait until it is gone, so the file is closed before it is deleted.
                    // It may have ended by itself just now. That is fine.
                    let _ = child.kill().await;
                    return Err(BACKUP_CANCELLED.into());
                }
                line = lines.next_segment() => match line? {
                    Some(line) => {
                        if log.read(&String::from_utf8_lossy(&line)) {
                            progress(log.progress(output, started));
                        }
                    }
                    None => break,
                },
                _ = tick.tick() => progress(log.progress(output, started)),
            }
        }
        let status = child.wait().await?;
        if status.success() {
            return Ok(());
        }
        let reason = log.reason();
        if reason.is_empty() {
            return Err(AppError::Message(format!(
                "The dump tool failed ({status})"
            )));
        }
        Err(AppError::Message(reason))
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
            // Names each table on stderr. That is how we show the progress.
            .arg("--verbose")
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
            .arg("--verbose")
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

/// The error output of a dump tool. With `--verbose` it names each table it starts.
/// It also keeps the lines we show when the tool fails.
#[derive(Default)]
struct DumpLog {
    /// The lines that are not progress chatter. Mostly the real errors.
    errors: Vec<String>,
    /// The last lines of all. We show them when no line looks like an error.
    tail: VecDeque<String>,
    table: Option<String>,
    tables: usize,
}

impl DumpLog {
    /// ponytail: a broken server can print many error lines. The first ones tell the cause.
    const MAX_ERRORS: usize = 20;
    const MAX_TAIL: usize = 5;

    /// Reads one line. Returns true when it names a new table.
    fn read(&mut self, line: &str) -> bool {
        let line = line.trim_end();
        if line.is_empty() {
            return false;
        }
        if self.tail.len() == Self::MAX_TAIL {
            self.tail.pop_front();
        }
        self.tail.push_back(line.to_owned());
        if let Some(table) = Self::table_of(line) {
            self.table = Some(table);
            self.tables += 1;
            return true;
        }
        if !Self::is_chatter(line) && self.errors.len() < Self::MAX_ERRORS {
            self.errors.push(line.to_owned());
        }
        false
    }

    /// The error text when the tool failed.
    fn reason(&self) -> String {
        if self.errors.is_empty() {
            Vec::from(self.tail.clone()).join("\n")
        } else {
            self.errors.join("\n")
        }
    }

    fn progress(&self, output: &Path, started: Instant) -> DumpProgress {
        DumpProgress {
            // The file is not there before the tool opens it.
            bytes: std::fs::metadata(output).map_or(0, |file| file.len()),
            seconds: started.elapsed().as_secs(),
            table: self.table.clone(),
            tables: self.tables,
        }
    }

    /// The table a line names, like `-- Retrieving table structure for table orders...`
    /// (mysqldump) or `pg_dump: dumping contents of table "public.orders"`.
    fn table_of(line: &str) -> Option<String> {
        let name = if let Some(rest) = line.strip_prefix("-- Retrieving table structure for table ")
        {
            rest.trim_end_matches("...").trim_matches('`')
        } else {
            line.strip_prefix("pg_dump: ")?
                .strip_prefix("dumping contents of table ")?
                .trim_matches('"')
        };
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// True for a `--verbose` line that only tells what the tool does now.
    fn is_chatter(line: &str) -> bool {
        if line.starts_with("-- ") {
            return true;
        }
        line.strip_prefix("pg_dump: ").is_some_and(|rest| {
            !["error:", "detail:", "hint:", "warning:"]
                .iter()
                .any(|kind| rest.starts_with(kind))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use futures_util::future::AbortHandle;

    use super::{DumpLog, DumpRepository};
    use crate::constants::BACKUP_CANCELLED;
    use crate::model::{ConnectionConfig, Driver, SslConfig};

    #[tokio::test]
    async fn a_cancel_leaves_the_older_backup_alone() {
        let folder = std::env::temp_dir();
        let id = std::process::id();
        let database = folder.join(format!("sqlpro-cancel-{id}.sqlite"));
        let _ = std::fs::remove_file(&database);
        // Enough rows that the dump still runs when the cancel comes.
        let filled = std::process::Command::new("sqlite3")
            .arg(&database)
            .arg(
                "CREATE TABLE t(x TEXT); WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL \
                 SELECT n + 1 FROM c WHERE n < 300000) INSERT INTO t SELECT hex(randomblob(64)) FROM c;",
            )
            .status();
        if !filled.is_ok_and(|status| status.success()) {
            println!("NOTE: the cancel check is skipped: sqlite3 was not found");
            return;
        }
        let path = folder.join(format!("sqlpro-cancel-{id}.sql"));
        std::fs::write(&path, "old").expect("old backup");
        let config = ConnectionConfig {
            driver: Driver::Sqlite,
            host: String::new(),
            port: 0,
            user: String::new(),
            password: String::new(),
            database: database.to_string_lossy().into_owned(),
            ssh: None,
            ssl: SslConfig::default(),
        };
        let (handle, registration) = AbortHandle::new_pair();
        let mut steps = 0;
        // The first progress comes right after the tool started. Cancel there.
        let cancel = |_| {
            steps += 1;
            handle.abort();
        };
        let result = DumpRepository::dump(&config, &path, cancel, registration).await;
        let old = std::fs::read_to_string(&path);
        let partial = std::fs::read_dir(&folder)
            .expect("temp folder")
            .flatten()
            .any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with(&format!("sqlpro-cancel-{id}.sql.")) && name.ends_with(".part")
            });
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&database);
        assert_eq!(
            result.expect_err("the dump was not cancelled").to_string(),
            BACKUP_CANCELLED
        );
        assert_eq!(steps, 1);
        assert_eq!(old.expect("old backup"), "old");
        assert!(!partial, "the side file is still there");
    }

    #[test]
    fn each_dump_has_its_own_side_file() {
        let path = Path::new("/backups/shop.sql");
        let first = DumpRepository::partial_path(path);
        let second = DumpRepository::partial_path(path);
        assert_ne!(first, second);
        for partial in [first, second] {
            assert_eq!(partial.extension(), Some(OsStr::new("part")));
            let name = partial.to_string_lossy().into_owned();
            assert!(name.starts_with("/backups/shop.sql."), "{name}");
        }
    }

    #[test]
    fn finds_the_table_of_mysqldump_and_pg_dump() {
        let mut log = DumpLog::default();
        assert!(!log.read("-- Connecting to 127.0.0.1..."));
        assert!(log.read("-- Retrieving table structure for table orders..."));
        assert_eq!(log.table.as_deref(), Some("orders"));
        assert!(!log.read("-- Sending SELECT query..."));
        assert!(log.read("-- Retrieving table structure for table `order items`...\n"));
        assert_eq!(log.table.as_deref(), Some("order items"));
        assert!(!log.read("pg_dump: reading schemas"));
        assert!(log.read(r#"pg_dump: dumping contents of table "public.users""#));
        assert_eq!(log.table.as_deref(), Some("public.users"));
        assert_eq!(log.tables, 3);
        // Chatter only: the error falls back to the last lines.
        assert!(
            log.reason()
                .ends_with("pg_dump: dumping contents of table \"public.users\"")
        );
    }

    #[test]
    fn keeps_only_the_real_errors() {
        let mut log = DumpLog::default();
        log.read("pg_dump: last built-in OID is 16383");
        log.read("pg_dump: reading extensions");
        log.read("pg_dump: error: query failed: ERROR:  permission denied for table secret");
        log.read("pg_dump: detail: Query was: LOCK TABLE public.secret IN ACCESS SHARE MODE");
        log.read("-- Retrieving rows...");
        log.read("mysqldump: Got error: 1045: Access denied for user 'me'");
        assert_eq!(
            log.reason(),
            "pg_dump: error: query failed: ERROR:  permission denied for table secret\n\
             pg_dump: detail: Query was: LOCK TABLE public.secret IN ACCESS SHARE MODE\n\
             mysqldump: Got error: 1045: Access denied for user 'me'"
        );
    }
}
