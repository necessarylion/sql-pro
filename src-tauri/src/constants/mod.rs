//! Global constants of the backend.

/// The most rows one result set keeps. More rows are dropped and the set is marked as cut.
pub const MAX_QUERY_ROWS: usize = 2000;

/// How long we wait for a connection before we give up.
pub const CONNECT_TIMEOUT_SECONDS: u64 = 10;

/// The name under which passwords are stored in the OS keychain.
pub const KEYCHAIN_SERVICE: &str = "SQL Pro";

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
