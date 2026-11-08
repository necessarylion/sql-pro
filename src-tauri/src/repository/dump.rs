//! Backups. We run the dump tool of each database: `mysqldump`, `pg_dump` and `sqlite3`.
//!
//! The tools know every detail of their own database, so the dump can be loaded again.
//! A custom format file of `PostgreSQL` is loaded again with `pg_restore`.

use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use futures_util::future::{AbortRegistration, Abortable};
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::Command;

use crate::constants::{
    BACKUP_CANCELLED, DUMP_PROGRESS_EVERY, MSSQL_NO_BACKUP, MYSQL_PASSWORD_ENV,
    PARTIAL_DUMP_SUFFIX, PG_CUSTOM_MAGIC, POSTGRES_PASSWORD_ENV, RESTORE_ONLY_POSTGRES,
};
#[cfg(unix)]
use crate::constants::{
    HOMEBREW_OPT_DIRS, LINUX_POSTGRES_DIR, POSTGRES_APP_VERSIONS_DIR, RHEL_POSTGRES_PARENT,
    TOOL_DIRS,
};
use crate::error::{AppError, AppResult};
use crate::model::{BackupOptions, ConnectionConfig, Driver, DumpProgress, PgTool, RestoreOptions};

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
        options: &BackupOptions,
        progress: impl FnMut(DumpProgress),
        cancel: AbortRegistration,
    ) -> AppResult<()> {
        if config.driver == Driver::Mssql {
            return Err(MSSQL_NO_BACKUP.into());
        }
        // The tool writes to a side file first. So a dump that fails leaves no
        // half file, and does not harm an older backup that is already at `path`.
        let partial = Self::partial_path(path);

        let mut result = match Self::command(config, &partial, options).await {
            Ok(command) => Self::run(command, &partial, DumpLog::default(), progress, cancel).await,
            Err(error) => Err(error),
        };
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

    /// Loads a custom format file of `pg_dump` into the current database with `pg_restore`.
    ///
    /// `pg_restore` goes on after an error. The restore then fails with the error lines
    /// and how many there were. Progress and cancel work like in `dump`. A cancel leaves
    /// what was loaded so far.
    pub async fn restore(
        config: &ConnectionConfig,
        file: &Path,
        options: &RestoreOptions,
        progress: impl FnMut(DumpProgress),
        cancel: AbortRegistration,
    ) -> AppResult<()> {
        if config.driver != Driver::Postgres {
            return Err(RESTORE_ONLY_POSTGRES.into());
        }
        if config.database.is_empty() {
            return Err("Choose a database first".into());
        }
        let tool = Self::pg_tool("pg_restore", options.version).await?;
        let command = Self::restore_command(&tool, config, file, options.clean);
        Self::run(command, file, DumpLog::restore(), progress, cancel).await
    }

    /// True when the file starts like a custom format file of `pg_dump`.
    pub fn is_custom_dump(path: &Path) -> AppResult<bool> {
        let mut start = [0; PG_CUSTOM_MAGIC.len()];
        // A file shorter than that is no custom dump.
        Ok(File::open(path)?.read_exact(&mut start).is_ok() && &start == PG_CUSTOM_MAGIC)
    }

    /// All versions of `name` (`pg_dump` or `pg_restore`) on this computer, newest first.
    /// One tool per version: the first one found wins.
    pub async fn pg_tools(name: &str) -> Vec<PgTool> {
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        let mut seen = HashSet::new();
        let mut tools: Vec<PgTool> = Vec::new();
        for folder in Self::folders() {
            let path = folder.join(&file);
            // PATH, Homebrew and Postgres.app often link to the same file.
            let Ok(real) = path.canonicalize() else {
                continue;
            };
            if !real.is_file() || !seen.insert(real) {
                continue;
            }
            if let Some(version) = Self::major_version(&path).await
                && !tools.iter().any(|tool| tool.version == version)
            {
                tools.push(PgTool { version, path });
            }
        }
        tools.sort_by_key(|tool| std::cmp::Reverse(tool.version));
        tools
    }

    /// The version to use by default: the one of the server, else the newest one.
    pub fn default_version(server: u32, tools: &[PgTool]) -> Option<u32> {
        tools
            .iter()
            .find(|tool| tool.version == server)
            .or(tools.first())
            .map(|tool| tool.version)
    }

    /// The tool `name` with this major version. None means the first one found.
    async fn pg_tool(name: &str, version: Option<u32>) -> AppResult<PathBuf> {
        let Some(version) = version else {
            return Self::find(&[name]).ok_or_else(|| {
                AppError::Message(format!(
                    "{name} was not found. Install the PostgreSQL client tools (macOS: brew install libpq)"
                ))
            });
        };
        Self::pg_tools(name)
            .await
            .into_iter()
            .find(|tool| tool.version == version)
            .map(|tool| tool.path)
            .ok_or_else(|| AppError::Message(format!("{name} {version} was not found")))
    }

    /// The major version a tool prints with `--version`. None when it does not run.
    async fn major_version(tool: &Path) -> Option<u32> {
        let mut command = Command::new(tool);
        super::hide_console(&mut command);
        let output = command
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .await
            .ok()?;
        Self::parse_major(&String::from_utf8_lossy(&output.stdout))
    }

    /// The major version in text like `pg_dump (PostgreSQL) 17.2 (Homebrew)`.
    fn parse_major(text: &str) -> Option<u32> {
        let word = text
            .split_whitespace()
            .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))?;
        let digits: String = word.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    }

    /// The `pg_restore` command. The password goes in the environment, never in an argument.
    fn restore_command(
        tool: &Path,
        config: &ConnectionConfig,
        file: &Path,
        clean: bool,
    ) -> Command {
        let mut command = Command::new(tool);
        command
            .arg(format!("--host={}", config.host))
            .arg(format!("--port={}", config.port))
            .arg("--no-password")
            .arg("--verbose")
            .arg(Self::dbname(&config.database))
            // The objects belong to the user of this connection, with no grants of
            // the old server. Its roles may not exist here.
            .arg("--no-owner")
            .arg("--no-privileges")
            .stdout(Stdio::null());
        if clean {
            command.arg("--clean").arg("--if-exists");
        }
        if !config.user.is_empty() {
            command.arg(format!("--username={}", config.user));
        }
        if !config.password.is_empty() {
            command.env(POSTGRES_PASSWORD_ENV, &config.password);
        }
        // "--" ends the options. A file name can then never be read as one.
        command.arg("--").arg(file);
        command
    }

    /// The `--dbname` option of `pg_dump` and `pg_restore`. They read a value with `=` or
    /// `postgresql://` as a whole connection string, which could name another host and get
    /// our password. So the name always goes in as `dbname='…'`, with `\` and `'` escaped.
    fn dbname(database: &str) -> String {
        let quoted = database.replace('\\', "\\\\").replace('\'', "\\'");
        format!("--dbname=dbname='{quoted}'")
    }

    /// The dump command of the database.
    async fn command(
        config: &ConnectionConfig,
        output: &Path,
        options: &BackupOptions,
    ) -> AppResult<Command> {
        match config.driver {
            Driver::Mysql => Self::mysql(config, output).await,
            Driver::Postgres => Self::postgres(config, output, options).await,
            Driver::Sqlite => Self::sqlite(config, output),
            Driver::Mssql => Err(MSSQL_NO_BACKUP.into()),
        }
    }

    /// Runs the tool and waits for it. Its error text becomes our error.
    /// `file` is the file whose size the progress shows.
    async fn run(
        mut command: Command,
        file: &Path,
        mut log: DumpLog,
        mut progress: impl FnMut(DumpProgress),
        cancel: AbortRegistration,
    ) -> AppResult<()> {
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
                            progress(log.progress(file, started));
                        }
                    }
                    None => break,
                },
                _ = tick.tick() => progress(log.progress(file, started)),
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

    async fn postgres(
        config: &ConnectionConfig,
        output: &Path,
        options: &BackupOptions,
    ) -> AppResult<Command> {
        let tool = Self::pg_tool("pg_dump", options.version).await?;
        let mut command = Command::new(tool);
        if options.custom {
            command.arg("--format=custom");
        }
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
            command.arg(Self::dbname(&config.database));
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
            // Each PostgreSQL version that is installed. The user picks one in the app.
            for homebrew in HOMEBREW_OPT_DIRS {
                for prefix in ["postgresql@", "libpq@"] {
                    folders.extend(Self::bin_folders(Path::new(homebrew), prefix));
                }
            }
            folders.extend(Self::bin_folders(Path::new(POSTGRES_APP_VERSIONS_DIR), ""));
            folders.extend(Self::bin_folders(Path::new(LINUX_POSTGRES_DIR), ""));
            folders.extend(Self::bin_folders(Path::new(RHEL_POSTGRES_PARENT), "pgsql-"));
        }
        // The installer of PostgreSQL for Windows. An absolute folder, so it is safe.
        #[cfg(windows)]
        {
            if let Some(programs) = std::env::var_os("ProgramFiles") {
                let installs = Path::new(&programs).join("PostgreSQL");
                folders.extend(Self::bin_folders(&installs, ""));
            }
        }
        folders
    }

    /// The `bin` folder of each folder in `parent` whose name starts with `prefix`.
    fn bin_folders(parent: &Path, prefix: &str) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(parent) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
            .map(|entry| entry.path().join("bin"))
            .collect()
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
    /// True for `pg_restore`. Its error lines are followed by the SQL that failed.
    restore: bool,
    /// How many `pg_restore: error:` lines there were.
    failures: usize,
}

impl DumpLog {
    fn restore() -> Self {
        Self {
            restore: true,
            ..Self::default()
        }
    }

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
        if line.starts_with("pg_restore: error:") {
            self.failures += 1;
        }
        // The SQL after a pg_restore error can be long. Only its first line stays.
        let shown =
            !self.restore || line.starts_with("pg_restore: ") || line.starts_with("Command was: ");
        if shown && !Self::is_chatter(line) && self.errors.len() < Self::MAX_ERRORS {
            self.errors.push(line.to_owned());
        }
        false
    }

    /// The error text when the tool failed.
    fn reason(&self) -> String {
        let lines = if self.errors.is_empty() {
            Vec::from(self.tail.clone()).join("\n")
        } else {
            self.errors.join("\n")
        };
        match self.failures {
            0 => lines,
            1 => format!("pg_restore had 1 error. The rest was restored.\n{lines}"),
            count => format!("pg_restore had {count} errors. The rest was restored.\n{lines}"),
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
    /// (mysqldump), `pg_dump: dumping contents of table "public.orders"` or
    /// `pg_restore: processing data for table "public.orders"`.
    fn table_of(line: &str) -> Option<String> {
        let name = if let Some(rest) = line.strip_prefix("-- Retrieving table structure for table ")
        {
            rest.trim_end_matches("...").trim_matches('`')
        } else if let Some(rest) = line.strip_prefix("pg_restore: ") {
            rest.strip_prefix("processing data for table ")?
                .trim_matches('"')
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
        let rest = line
            .strip_prefix("pg_dump: ")
            .or_else(|| line.strip_prefix("pg_restore: "));
        rest.is_some_and(|rest| {
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
    use crate::model::{BackupOptions, ConnectionConfig, Driver, PgTool, SslConfig};

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
        let options = BackupOptions::default();
        let result = DumpRepository::dump(&config, &path, &options, cancel, registration).await;
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

    fn postgres(password: &str) -> ConnectionConfig {
        ConnectionConfig {
            driver: Driver::Postgres,
            host: "127.0.0.1".to_owned(),
            port: 5433,
            user: "me".to_owned(),
            password: password.to_owned(),
            database: "shop".to_owned(),
            ssh: None,
            ssl: SslConfig::default(),
        }
    }

    #[test]
    fn reads_the_major_version_of_the_tools() {
        let parse = DumpRepository::parse_major;
        assert_eq!(parse("pg_dump (PostgreSQL) 17.2\n"), Some(17));
        assert_eq!(parse("pg_restore (PostgreSQL) 15.10 (Homebrew)"), Some(15));
        assert_eq!(parse("pg_dump (PostgreSQL) 18beta1"), Some(18));
        assert_eq!(parse("pg_dump (PostgreSQL) 9.6.24"), Some(9));
        assert_eq!(parse("command not found"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn the_default_version_is_the_one_of_the_server() {
        let tool = |version| PgTool {
            version,
            path: format!("/pg{version}/pg_dump").into(),
        };
        let tools = [tool(18), tool(17), tool(15)];
        assert_eq!(DumpRepository::default_version(17, &tools), Some(17));
        // No tool of the server version: the newest one.
        assert_eq!(DumpRepository::default_version(16, &tools), Some(18));
        assert_eq!(DumpRepository::default_version(17, &[]), None);
    }

    #[test]
    fn the_restore_password_is_never_an_argument() {
        let file = Path::new("/backups/-shop.dump");
        let tool = Path::new("pg_restore");
        let command = DumpRepository::restore_command(tool, &postgres("s3cret"), file, true);
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--host=127.0.0.1",
                "--port=5433",
                "--no-password",
                "--verbose",
                "--dbname=dbname='shop'",
                "--no-owner",
                "--no-privileges",
                "--clean",
                "--if-exists",
                "--username=me",
                "--",
                "/backups/-shop.dump",
            ]
        );
        assert!(args.iter().all(|arg| !arg.contains("s3cret")));
        let password = command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == "PGPASSWORD")
            .and_then(|(_, value)| value);
        assert_eq!(password, Some(OsStr::new("s3cret")));
        // Without clean and without a password.
        let command = DumpRepository::restore_command(tool, &postgres(""), file, false);
        let args: Vec<_> = command.as_std().get_args().collect();
        assert!(!args.contains(&OsStr::new("--clean")));
        assert_eq!(command.as_std().get_envs().count(), 0);
    }

    #[test]
    fn a_database_name_is_never_a_connection_string() {
        let dbname = DumpRepository::dbname;
        assert_eq!(dbname("shop"), "--dbname=dbname='shop'");
        assert_eq!(
            dbname("x host=evil.example"),
            "--dbname=dbname='x host=evil.example'"
        );
        assert_eq!(
            dbname("postgresql://evil.example/x"),
            "--dbname=dbname='postgresql://evil.example/x'"
        );
        // A quote or backslash cannot end the value early.
        assert_eq!(
            dbname(r"it's' host=evil \"),
            r"--dbname=dbname='it\'s\' host=evil \\'"
        );
    }

    #[test]
    fn knows_a_custom_dump_by_its_first_bytes() {
        let folder = std::env::temp_dir();
        let id = std::process::id();
        let custom = folder.join(format!("sqlpro-magic-{id}.dump"));
        let plain = folder.join(format!("sqlpro-magic-{id}.sql"));
        let short = folder.join(format!("sqlpro-magic-{id}.short"));
        std::fs::write(&custom, b"PGDMP\x01\x0e\x00").expect("custom");
        std::fs::write(&plain, "-- PostgreSQL database dump\n").expect("plain");
        std::fs::write(&short, "PG").expect("short");
        let found = [&custom, &plain, &short].map(|path| DumpRepository::is_custom_dump(path));
        for path in [&custom, &plain, &short] {
            let _ = std::fs::remove_file(path);
        }
        assert_eq!(
            found.map(|found| found.expect("read")),
            [true, false, false]
        );
    }

    #[test]
    fn counts_the_errors_of_pg_restore() {
        let mut log = DumpLog::restore();
        assert!(!log.read("pg_restore: connecting to database for restore"));
        assert!(!log.read(r#"pg_restore: creating TABLE "public.orders""#));
        assert!(log.read(r#"pg_restore: processing data for table "public.orders""#));
        assert_eq!(log.table.as_deref(), Some("public.orders"));
        log.read(r#"pg_restore: error: could not execute query: ERROR:  relation "orders" already exists"#);
        log.read("Command was: CREATE TABLE public.orders (");
        log.read("    id integer NOT NULL");
        log.read(");");
        log.read(
            r#"pg_restore: error: could not execute query: ERROR:  role "old" does not exist"#,
        );
        log.read("pg_restore: warning: errors ignored on restore: 2");
        assert_eq!(
            log.reason(),
            "pg_restore had 2 errors. The rest was restored.\n\
             pg_restore: error: could not execute query: ERROR:  relation \"orders\" already exists\n\
             Command was: CREATE TABLE public.orders (\n\
             pg_restore: error: could not execute query: ERROR:  role \"old\" does not exist\n\
             pg_restore: warning: errors ignored on restore: 2"
        );
    }
}
