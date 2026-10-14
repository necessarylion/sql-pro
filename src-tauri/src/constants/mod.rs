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

/// The error for a database kind that has no accounts.
pub const NO_USER_ACCOUNTS: &str = "SQLite has no user accounts";

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
