//! Global constants of the backend.

/// The most rows one result set keeps. More rows are dropped and the set is marked as cut.
pub const MAX_QUERY_ROWS: usize = 2000;

/// How many rows of a CSV file the import dialog shows. The header row counts too.
pub const CSV_PREVIEW_ROWS: usize = 6;

/// How long we wait for a connection before we give up.
pub const CONNECT_TIMEOUT_SECONDS: u64 = 10;

/// Which build this is: `dev` for a debug build (`tauri dev` and `tauri build --debug`),
/// empty for a release build. The two builds must never share saved data.
pub const PROFILE: &str = if cfg!(debug_assertions) { "dev" } else { "" };

/// The name of the plugin that tells the frontend the profile.
pub const PROFILE_PLUGIN: &str = "profile";

/// The frontend reads the profile from this global: `window.__SQL_PRO_PROFILE__`.
pub const PROFILE_GLOBAL: &str = "__SQL_PRO_PROFILE__";

/// The name under which passwords are stored in the OS keychain.
/// A debug build has its own name, so it can never read, change or delete
/// the passwords of the release app. The same holds the other way round.
pub const KEYCHAIN_SERVICE: &str = if cfg!(debug_assertions) {
    "SQL Pro (dev)"
} else {
    "SQL Pro"
};

/// The account name of the one keychain item that holds all passwords.
/// One item means one permission prompt. No password id may have this name.
pub const KEYCHAIN_VAULT_ACCOUNT: &str = "sql-pro-vault";

/// How long we wait for an SSH tunnel to come up.
pub const SSH_TIMEOUT_SECONDS: u64 = 20;

/// The environment variable that carries the SSH password to the askpass helper.
pub const SSH_PASSWORD_ENV: &str = "SQLPRO_SSH_PASSWORD";

/// Where the local end of an SSH tunnel listens.
pub const LOCALHOST: &str = "127.0.0.1";

/// The escape character in LIKE patterns. It is not the backslash, because
/// `MySQL` also reads a backslash as an escape inside a text value.
pub const LIKE_ESCAPE: char = '!';

/// The environment variable that carries the password to `mysqldump`.
pub const MYSQL_PASSWORD_ENV: &str = "MYSQL_PWD";

/// The environment variable that carries the password to `pg_dump`.
pub const POSTGRES_PASSWORD_ENV: &str = "PGPASSWORD";

/// Where we look for the dump tools, after the folders of PATH.
/// An app started from the Finder has a very short PATH.
/// Used on macOS and Linux only. Windows has no such folders.
pub const TOOL_DIRS: [&str; 8] = [
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/opt/homebrew/opt/mysql-client/bin",
    "/opt/homebrew/opt/libpq/bin",
    "/usr/local/opt/mysql-client/bin",
    "/usr/local/opt/libpq/bin",
    "/Applications/Postgres.app/Contents/Versions/latest/bin",
];

/// Homebrew keeps each `PostgreSQL` version in a folder like `postgresql@16` in here.
pub const HOMEBREW_OPT_DIR: &str = "/opt/homebrew/opt";

/// The Windows flag `CREATE_NO_WINDOW`. A tool started with it opens no console window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What we add to the path of a dump while it is being written.
pub const PARTIAL_DUMP_SUFFIX: &str = ".part";

/// The `MySQL` account host that means "from any host".
pub const ANY_HOST: &str = "%";

/// The longest `MySQL` user name, in characters.
pub const MYSQL_USER_NAME_MAX: usize = 32;

/// The longest `MySQL` account host, in characters.
pub const MYSQL_HOST_MAX: usize = 255;

/// The longest `PostgreSQL` role name, in bytes. The server cuts longer names silently.
pub const POSTGRES_ROLE_NAME_MAX: usize = 63;

/// Accounts that `MySQL` and `MariaDB` create for their own use.
pub const MYSQL_SYSTEM_USERS: [&str; 4] = [
    "mysql.sys",
    "mysql.session",
    "mysql.infoschema",
    "mariadb.sys",
];

/// Every right that `GRANT ALL` on one `MySQL` database gives.
/// An account that holds all of them can do everything in that database.
pub const MYSQL_DATABASE_PRIVILEGES: [&str; 18] = [
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "CREATE",
    "DROP",
    "REFERENCES",
    "INDEX",
    "ALTER",
    "CREATE TEMPORARY TABLES",
    "LOCK TABLES",
    "EXECUTE",
    "CREATE VIEW",
    "SHOW VIEW",
    "CREATE ROUTINE",
    "ALTER ROUTINE",
    "EVENT",
    "TRIGGER",
];

/// The error for an account that may not create, change or drop users.
pub const CANNOT_MANAGE_USERS: &str = "Your account is not allowed to manage users";

/// The error when the query stats right comes from a wider right that the app cannot take away.
pub const QUERY_STATS_FIXED: &str = "This account reads the query stats through a wider right, \
    like a superuser or SELECT on all databases. It cannot be changed here.";

/// The error for a database kind that has no accounts.
pub const NO_USER_ACCOUNTS: &str = "SQLite has no user accounts";

/// The errors for features that SQL Server does not have in the app yet.
pub const MSSQL_NO_USER_ACCOUNTS: &str = "User accounts are not supported for SQL Server yet";
pub const MSSQL_NO_BACKUP: &str = "Backups are not supported for SQL Server yet";

/// The errors for creating and dropping a database.
pub const SQLITE_NO_DATABASES: &str = "SQLite has no databases to create or drop";
pub const CHARSET_ONLY_MYSQL: &str = "Charset and collation are only for MySQL";
pub const DROP_CURRENT_DATABASE: &str = "Switch to another database before you drop this one";

/// The error for a database kind that has no server processes.
pub const NO_PROCESSES: &str = "SQLite has no server processes";

/// The error for a database kind that has no query stats.
pub const NO_QUERY_STATS: &str = "SQLite has no query stats";

/// The Query stats view shows at most this many query shapes, the slowest first.
pub const QUERY_STATS_LIMIT: u32 = 500;

/// The bounds of "last N days" in the Query stats view.
pub const QUERY_STATS_MIN_DAYS: u32 = 1;
pub const QUERY_STATS_MAX_DAYS: u32 = 3650;

/// What to do when the server does not collect query stats, or the account may not read them.
pub const MYSQL_NO_QUERY_STATS: &str = "Query stats need performance_schema, and it is off or not readable. \
    Set performance_schema=ON in my.cnf and restart the server. \
    Your account also needs SELECT on performance_schema. \
    An admin can give it on the Users screen (Query stats).";
pub const POSTGRES_NO_QUERY_STATS: &str = "Query stats need the pg_stat_statements extension. \
    Add pg_stat_statements to shared_preload_libraries, restart the server, \
    then run CREATE EXTENSION pg_stat_statements; in this database. \
    To see the queries of other users, your account needs the pg_read_all_stats role. \
    An admin can give it on the Users screen (Query stats).";
pub const MSSQL_NO_QUERY_STATS: &str = "Query stats need the VIEW SERVER STATE permission \
    (on Azure SQL: VIEW DATABASE STATE).";

/// The error when the process to stop is not in the list any more.
pub const PROCESS_GONE: &str = "This process is not running any more";

/// The error for a foreign key change on `SQLite`.
pub const SQLITE_FOREIGN_KEYS_FIXED: &str =
    "SQLite cannot add or drop a foreign key of an existing table";

/// The error for a try to drop the primary key as an index.
pub const PRIMARY_KEY_NOT_DROPPED: &str = "The primary key cannot be dropped here";

/// Shown in place of a database message that could hold a password.
pub const PASSWORD_ERROR_HIDDEN: &str = "The database refused the statement. The details are hidden because they could show the password.";

/// The file name the program runs under in dev on macOS. The Dock shows it.
pub const DEV_APP_NAME: &str = "SQL Pro";

/// The label of the one window of the app. Tauri gives it this name.
pub const MAIN_WINDOW: &str = "main";

/// The event that tells the frontend that the OS gave the app files to open.
/// The frontend then takes them with the `take_opened_files` command.
pub const OPEN_FILES_EVENT: &str = "open-files";

/// The id of the tray icon. Windows and Linux only: macOS has the Dock.
pub const TRAY_ID: &str = "main";

/// The tray menu item that shows the window again.
pub const TRAY_SHOW_ID: &str = "show";
pub const TRAY_SHOW_TEXT: &str = "Show SQL Pro";

/// The tray menu item that quits the app for real.
pub const TRAY_QUIT_ID: &str = "quit";
pub const TRAY_QUIT_TEXT: &str = "Quit";

/// The title of the popup that offers a new version.
pub const UPDATE_TITLE: &str = "Update available";

/// The two buttons of that popup.
pub const UPDATE_INSTALL_TEXT: &str = "Install and restart";
pub const UPDATE_LATER_TEXT: &str = "Later";

/// The title of the popup that says the update could not be installed.
pub const UPDATE_ERROR_TITLE: &str = "Update failed";

/// The error of a query that the user cancelled.
pub const QUERY_CANCELLED: &str = "Query cancelled";

/// The error when the cancel cannot reach the server in time.
pub const CANCEL_NOT_REACHED: &str = "Could not reach the server to cancel the query";

/// The `kind` of a `DbObject`. A view is also a `TableRef` with the kind `view`.
pub const VIEW: &str = "view";
pub const TRIGGER: &str = "trigger";
pub const PROCEDURE: &str = "procedure";
pub const FUNCTION: &str = "function";

/// How many `SQLite` steps run between two checks for a cancel.
/// A smaller number reacts faster but makes every query a little slower.
pub const SQLITE_CANCEL_CHECK_STEPS: i32 = 1000;

/// Where `claude` may be, inside the home folder. Tried first on macOS and Linux.
pub const CLAUDE_HOME_PATHS: [&str; 3] = [
    ".claude/local/claude",
    ".local/bin/claude",
    ".npm-global/bin/claude",
];

/// Where `claude.exe` may be on Windows, after `where claude`.
/// Each entry is an environment variable and the path inside that folder.
pub const CLAUDE_WINDOWS_PATHS: [(&str, &str); 3] = [
    ("USERPROFILE", r".local\bin\claude.exe"),
    ("LOCALAPPDATA", r"Programs\claude\claude.exe"),
    ("APPDATA", r"npm\claude.cmd"),
];

/// The script of the npm package, next to the npm `claude.cmd` shim. We run it with
/// `node`, because a `.cmd` file cannot take our long arguments safely.
pub const CLAUDE_NPM_SCRIPT: &str = r"node_modules\@anthropic-ai\claude-code\cli.js";

/// The error on Windows when only the npm shim is there and `node` or its script is missing.
pub const CLAUDE_NEEDS_NATIVE: &str = "Claude Code was found only as an npm script that SQL Pro cannot run. Install the native build from https://claude.com/claude-code (claude.exe).";

/// How long one answer of the AI may take. Then the run is killed like a stop.
pub const CLAUDE_TIMEOUT_SECONDS: u64 = 300;

/// The error when the AI did not answer in `CLAUDE_TIMEOUT_SECONDS`.
pub const CLAUDE_TIMEOUT: &str = "The AI did not answer in 5 minutes.";

/// The error when an AI read would run while the tab has an open transaction.
/// The read-only guard would end (MySQL: commit, others: roll back) the user's work.
pub const OPEN_TRANSACTION: &str = "Finish the open transaction in the Query view first.";

/// The largest image the AI may get, in bytes.
pub const AI_IMAGE_MAX_BYTES: usize = 5 * 1024 * 1024;

/// The largest CSV file the AI may get, in bytes. A bigger one is refused, never cut.
pub const AI_CSV_MAX_BYTES: usize = 200 * 1024;

/// The most images, and the most CSV files, in one message.
pub const AI_MAX_IMAGES: usize = 5;
pub const AI_MAX_CSV_FILES: usize = 5;

/// The error for a file type the AI may not get.
pub const AI_FILE_TYPES: &str = "Only PNG, JPEG and WebP images and CSV files can be attached.";

/// The error when SQL that is not a read is sent to the read-only path.
pub const NOT_A_READ: &str = "This SQL is not a read, so it cannot run as one.";

/// The first words of a statement that Explain takes. Anything else is refused,
/// so `ANALYZE` (which runs the statement) can never come first.
pub const EXPLAIN_KEYWORDS: [&str; 9] = [
    "SELECT", "WITH", "INSERT", "UPDATE", "DELETE", "REPLACE", "VALUES", "TABLE", "MERGE",
];

/// The error when Explain gets text that is not one plain statement.
pub const EXPLAIN_ONE_STATEMENT: &str =
    "Explain takes one SELECT, INSERT, UPDATE or DELETE statement.";

/// The error when the database has no Explain in this app.
pub const NO_EXPLAIN: &str = "Explain is not available for this database.";

/// The error when the Claude Code CLI cannot be found.
pub const CLAUDE_NOT_FOUND: &str = "Claude Code is not installed. Install it from https://claude.com/claude-code, then run `claude` once in a terminal to log in.";

/// The error of a Claude run that the user stopped.
pub const CLAUDE_STOPPED: &str = "Stopped";

/// The longest database description we send to Claude, in characters.
pub const CLAUDE_SCHEMA_MAX_CHARS: usize = 40_000;

/// The shape every reply of Claude must have. The CLI checks it (`--json-schema`).
pub const CLAUDE_REPLY_SCHEMA: &str = r#"{"type":"object","properties":{"sql":{"type":["string","null"]},"message":{"type":"string"}},"required":["sql","message"]}"#;

/// The rules for Claude. The database kind and tables come with the first question.
pub const CLAUDE_SYSTEM_PROMPT: &str = "You help a user work with one SQL database inside a desktop SQL client. \
You cannot run anything yourself. The app runs SQL for you.\n\
Always reply with JSON only: {\"sql\": string or null, \"message\": string}.\n\
- To look at or change data, put exactly ONE SQL statement in \"sql\". Use the exact SQL dialect of the database you are told. \
The app runs it and sends you the result (or the error) in the next message. Then reply again.\n\
- Look data up with SELECT before you change it. For example, find a user by email before you block that user.\n\
- Change only what the user asked for. The user must confirm every change.\n\
- When you have the answer, or nothing more needs to run, set \"sql\" to null and answer the user in \"message\".\n\
- \"message\" is short, plain English for the user. Say what the SQL does when you send SQL.\n\
- If a query was rejected by the user, do not try to run it again in another way. Ask what they want.";

/// The AI tools the app can run. Only Claude Code for now.
pub const AI_PROVIDERS: [&str; 1] = ["claude"];

/// The `--model` aliases of `claude`. A full model id is checked with `MODEL_ID_MAX` and
/// the allowed characters instead. No value means the default model.
pub const CLAUDE_MODEL_ALIASES: [&str; 4] = ["opus", "sonnet", "haiku", "fable"];

/// The longest model id we pass to `claude`.
pub const MODEL_ID_MAX: usize = 80;

/// The `--effort` values we pass to `claude`. No value means the default effort.
pub const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Goes in front of every message in read-only mode.
pub const CLAUDE_READ_ONLY_NOTE: &str = "This session is READ-ONLY. Only reads (SELECT and the like) will run. \
Write queries are not allowed in read-only mode, so do not send them.";

/// The error when `--resume` cannot find the session any more. The frontend knows this text.
pub const CLAUDE_SESSION_GONE: &str = "The AI session is gone";

/// The folder in the app data folder that `claude` runs in. `--resume` finds a session
/// by this folder, so it must stay. A debug build has its own folder.
pub const CLAUDE_FOLDER: &str = if cfg!(debug_assertions) {
    "claude-dev"
} else {
    "claude"
};

/// A statement that starts with one of these words can be a read.
pub const READ_KEYWORDS: [&str; 6] = ["SELECT", "SHOW", "EXPLAIN", "DESCRIBE", "DESC", "WITH"];

/// A read may not hold any of these words. Each one can change data or settings.
pub const WRITE_KEYWORDS: [&str; 17] = [
    "INSERT", "UPDATE", "DELETE", "MERGE", "REPLACE", "ALTER", "DROP", "CREATE", "TRUNCATE",
    "GRANT", "REVOKE", "INTO", "CALL", "EXEC", "EXECUTE", "SET", "LOCK",
];

#[cfg(test)]
mod tests {
    use super::{KEYCHAIN_SERVICE, PROFILE};

    /// Run with and without `--release` to check both builds.
    #[test]
    fn debug_and_release_keep_their_data_apart() {
        let debug = cfg!(debug_assertions);
        assert_eq!(PROFILE == "dev", debug);
        assert_eq!(PROFILE.is_empty(), !debug);
        assert_eq!(KEYCHAIN_SERVICE == "SQL Pro (dev)", debug);
        assert_eq!(KEYCHAIN_SERVICE == "SQL Pro", !debug);
    }
}
