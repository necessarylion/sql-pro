//! Database access. One repository per driver, all behind the same trait.

mod dump;
mod mysql;
mod password;
mod postgres;
mod sqlite;
mod tunnel;

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use sqlx::pool::PoolOptions;
use sqlx::{
    AssertSqlSafe, Column as _, Database, Either, Encode, Executor, IntoArguments, Pool, Row as _,
    Type,
};

pub use dump::DumpRepository;
pub use password::{Keychain, OsKeychain, PasswordRepository};
pub use tunnel::SshTunnel;

use crate::constants::{
    CONNECT_TIMEOUT_SECONDS, LOCALHOST, MAX_QUERY_ROWS, NO_USER_ACCOUNTS, PASSWORD_ERROR_HIDDEN,
};
use crate::error::{AppError, AppResult};
use crate::model::{
    AccessLevel, ColumnInfo, ColumnSpec, ConnectionConfig, Driver, Relation, ResultSet, Row,
    TableRef, UserList,
};

/// Stops the console window that Windows opens for every tool a window app starts.
/// Other systems open no such window, so nothing happens there.
fn hide_console(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    command.creation_flags(crate::constants::CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = command;
}

/// Everything the service needs from one open database connection.
#[async_trait]
pub trait Repository: Send + Sync {
    /// Wraps a table or column name so it is safe inside SQL.
    fn quote(&self, identifier: &str) -> String;

    /// The marker for bound value number `index` (starts at 1) of a column with `data_type`.
    fn placeholder(&self, index: usize, data_type: &str) -> String;

    /// Writes a text value as a SQL string. The value cannot break out of the quotes.
    fn literal(&self, value: &str) -> String;

    /// A quoted column in a form that LIKE accepts for every column type.
    fn like_operand(&self, column: &str) -> String {
        column.to_owned()
    }

    /// The tail of an INSERT that sets no column at all.
    fn default_values(&self) -> &'static str;

    /// Runs raw SQL. It may hold many statements. Gives one result set per statement.
    async fn query(&self, sql: &str) -> AppResult<Vec<ResultSet>>;

    /// Runs one statement with bound values. Gives the number of changed rows.
    async fn execute(&self, sql: &str, params: Vec<Option<String>>) -> AppResult<u64>;

    /// Runs the statements in one transaction, in the given order. All or nothing.
    ///
    /// Each statement may change at most one row. When one fails or would change
    /// more, everything is rolled back and the error names the statement.
    /// Gives the number of changed rows of each statement.
    async fn execute_all(&self, statements: Vec<Statement>) -> AppResult<Vec<u64>>;

    /// Runs raw SQL statements in one transaction, in the given order. Their bound values
    /// are not used. When one fails, everything is rolled back and the error names it.
    async fn run_script(&self, statements: Vec<Statement>) -> AppResult<()>;

    async fn databases(&self) -> AppResult<Vec<String>>;

    async fn tables(&self) -> AppResult<Vec<TableRef>>;

    async fn columns(&self, table: &TableRef) -> AppResult<Vec<ColumnInfo>>;

    /// All foreign keys of the current database.
    async fn relations(&self) -> AppResult<Vec<Relation>>;

    /// The statement that creates `table`, as the database itself writes it.
    /// `None` when the database cannot give one. Then the caller builds it from the columns.
    async fn create_sql(&self, _table: &TableRef) -> AppResult<Option<String>> {
        Ok(None)
    }

    async fn close(&self);

    /// The statements that change the type, null rule and default of a column.
    /// A rename is not part of this. The service does it after.
    async fn modify_column_sql(
        &self,
        table: &TableRef,
        old: &ColumnInfo,
        new: &ColumnSpec,
    ) -> AppResult<Vec<String>>;

    /// The login accounts of the server, and what each may do in the current database.
    /// The default is for a database without accounts: an empty list.
    async fn users(&self) -> AppResult<UserList> {
        Ok(UserList::default())
    }

    /// Fails when an access level cannot be set right now.
    async fn check_grantable(&self) -> AppResult<()> {
        Ok(())
    }

    /// Creates a login account with no rights. `host` is only used by `MySQL`.
    /// The caller has checked the texts for control characters.
    async fn create_user(
        &self,
        _name: &str,
        _host: Option<&str>,
        _password: &str,
    ) -> AppResult<()> {
        Err(NO_USER_ACCOUNTS.into())
    }

    /// Removes a login account.
    async fn drop_user(&self, _name: &str, _host: Option<&str>) -> AppResult<()> {
        Err(NO_USER_ACCOUNTS.into())
    }

    /// Gives an account a new password.
    async fn set_user_password(
        &self,
        _name: &str,
        _host: Option<&str>,
        _password: &str,
    ) -> AppResult<()> {
        Err(NO_USER_ACCOUNTS.into())
    }

    /// Makes the rights of an account on the current database exactly `access`:
    /// it removes what the account has here, then grants the new level.
    /// Rights on other databases and global rights stay as they are.
    async fn set_user_access(
        &self,
        _name: &str,
        _host: Option<&str>,
        _access: AccessLevel,
    ) -> AppResult<()> {
        Err(NO_USER_ACCOUNTS.into())
    }

    /// The statement that deletes all rows of a table. `table` is already quoted.
    fn truncate_sql(&self, table: &str) -> String {
        format!("TRUNCATE TABLE {table}")
    }

    /// The statement that renames a table. Both names are already quoted.
    fn rename_table_sql(&self, table: &str, new_name: &str) -> String {
        format!("ALTER TABLE {table} RENAME TO {new_name}")
    }

    /// The words that make a primary key column number its rows itself.
    /// They go right after `PRIMARY KEY` in a `CREATE TABLE`.
    fn auto_increment(&self) -> &'static str;

    /// The full, quoted name of a table.
    fn table_name(&self, table: &TableRef) -> String {
        match &table.schema {
            Some(schema) => format!("{}.{}", self.quote(schema), self.quote(&table.name)),
            None => self.quote(&table.name),
        }
    }
}

/// One statement of a batch, with its bound values.
pub struct Statement {
    /// What the error calls this statement, like `Row 3`.
    pub label: String,
    pub sql: String,
    pub params: Vec<Option<String>>,
}

/// The error for a change of a batch that failed.
pub fn not_saved(label: &str, error: &AppError) -> AppError {
    AppError::Message(format!("{label} could not be saved: {error}"))
}

/// Opens the right repository for a connection config.
pub struct RepositoryFactory;

impl RepositoryFactory {
    /// Also opens the SSH tunnel when the config asks for one.
    /// The caller must keep the tunnel alive as long as the repository.
    pub async fn create(
        &self,
        config: &ConnectionConfig,
    ) -> AppResult<(Arc<dyn Repository>, Option<SshTunnel>)> {
        let mut tunnel = None;
        let mut tunneled;
        let mut config = config;
        if let (Some(ssh), true) = (&config.ssh, config.driver != Driver::Sqlite) {
            let opened = SshTunnel::open(ssh, &config.host, config.port).await?;
            tunneled = config.clone();
            LOCALHOST.clone_into(&mut tunneled.host);
            tunneled.port = opened.local_port();
            tunnel = Some(opened);
            config = &tunneled;
        }
        let repository: Arc<dyn Repository> = match config.driver {
            Driver::Mysql => Arc::new(mysql::MysqlRepository::connect(config).await?),
            Driver::Postgres => Arc::new(postgres::PostgresRepository::connect(config).await?),
            Driver::Sqlite => Arc::new(sqlite::SqliteRepository::connect(config).await?),
        };
        Ok((repository, tunnel))
    }
}

/// Pool settings shared by all drivers.
///
/// We keep exactly one connection per tab. This way session state
/// (transactions, `SET`, temp tables) stays the same between queries.
// ponytail: one connection per tab, so a slow query blocks that tab. Add a second
// connection for browsing if that hurts.
fn pool_options<DB: Database>() -> PoolOptions<DB> {
    PoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .acquire_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
}

/// Runs the statements in one transaction on the one connection of the pool.
/// See `Repository::execute_all`.
// ponytail: a transaction that the user opened by hand in the query editor is
// not seen here. MySQL commits it first, PostgreSQL commits it with the batch,
// and SQLite refuses the batch. Track open transactions if that hurts.
async fn execute_all<DB>(
    pool: &Pool<DB>,
    statements: Vec<Statement>,
    affected: fn(&DB::QueryResult) -> u64,
) -> AppResult<Vec<u64>>
where
    DB: Database,
    for<'conn> &'conn mut DB::Connection: Executor<'conn, Database = DB>,
    DB::Arguments: IntoArguments<DB>,
    for<'args> Option<String>: Encode<'args, DB> + Type<DB>,
{
    let mut transaction = pool.begin().await?;
    let mut counts = Vec::with_capacity(statements.len());
    for statement in statements {
        let mut query = sqlx::query(AssertSqlSafe(statement.sql));
        for param in statement.params {
            query = query.bind(param);
        }
        let error = match query.execute(&mut *transaction).await {
            Ok(result) if affected(&result) <= 1 => {
                counts.push(affected(&result));
                continue;
            }
            Ok(result) => AppError::Message(format!("it would change {} rows", affected(&result))),
            Err(error) => error.into(),
        };
        // A failed rollback closes the connection, and that rolls back too.
        let _ = transaction.rollback().await;
        return Err(not_saved(&statement.label, &error));
    }
    transaction.commit().await?;
    Ok(counts)
}

/// Runs raw statements in one transaction. See `Repository::run_script`.
///
/// `MySQL` commits by itself before and after each `CREATE`, `DROP` or `ALTER`.
/// So there a failed script keeps what ran up to the last such statement.
async fn run_script<DB>(pool: &Pool<DB>, statements: Vec<Statement>) -> AppResult<()>
where
    DB: Database,
    for<'conn> &'conn mut DB::Connection: Executor<'conn, Database = DB>,
{
    let mut transaction = pool.begin().await?;
    for statement in statements {
        let run = sqlx::raw_sql(AssertSqlSafe(statement.sql))
            .execute(&mut *transaction)
            .await;
        if let Err(error) = run {
            // A failed rollback closes the connection, and that rolls back too.
            let _ = transaction.rollback().await;
            let error = AppError::from(error);
            return Err(AppError::Message(format!("{}: {error}", statement.label)));
        }
    }
    transaction.commit().await?;
    Ok(())
}

/// What sqlx streams back: rows, and one statement result after each statement.
type Items<'conn, DB> = BoxStream<
    'conn,
    Result<Either<<DB as Database>::QueryResult, <DB as Database>::Row>, sqlx::Error>,
>;

/// Reads a stream of rows and statement results into result sets.
///
/// `read` turns one cell into text. `affected` reads the changed row count.
// ponytail: a SELECT with zero rows gives no column names, because sqlx only
// exposes columns on a row. Use `describe` if empty results need headers.
async fn collect<DB: Database>(
    mut stream: Items<'_, DB>,
    read: fn(&DB::Row, usize) -> Result<Option<String>, sqlx::Error>,
    affected: fn(&DB::QueryResult) -> u64,
) -> AppResult<Vec<ResultSet>> {
    let mut sets = Vec::new();
    let mut current = ResultSet::default();
    while let Some(item) = stream.next().await {
        match item? {
            Either::Left(result) => {
                current.affected = affected(&result);
                sets.push(std::mem::take(&mut current));
            }
            Either::Right(row) => {
                if current.columns.is_empty() {
                    for column in row.columns() {
                        current.columns.push(column.name().to_owned());
                    }
                }
                // We still read past the limit so the connection stays clean, but we drop the rows.
                if current.rows.len() >= MAX_QUERY_ROWS {
                    current.truncated = true;
                    continue;
                }
                let mut cells = Vec::with_capacity(row.len());
                for index in 0..row.len() {
                    cells.push(read(&row, index)?);
                }
                current.rows.push(cells);
            }
        }
    }
    if !current.columns.is_empty() {
        sets.push(current);
    }
    Ok(sets)
}

/// The result of the last statement, or an empty one.
fn single(mut sets: Vec<ResultSet>) -> ResultSet {
    sets.pop().unwrap_or_default()
}

/// The text of one cell. NULL and missing cells become an empty string.
fn cell(row: &Row, index: usize) -> String {
    row.get(index).cloned().flatten().unwrap_or_default()
}

/// The first cell of every row.
fn first_column(set: &ResultSet) -> Vec<String> {
    let mut values = Vec::with_capacity(set.rows.len());
    for row in &set.rows {
        values.push(cell(row, 0));
    }
    values
}

/// Builds columns from rows shaped as: name, type, nullable (YES/NO), default, key (PRI), extra.
fn standard_columns(set: &ResultSet) -> Vec<ColumnInfo> {
    let mut columns = Vec::with_capacity(set.rows.len());
    for row in &set.rows {
        columns.push(ColumnInfo {
            name: cell(row, 0),
            data_type: cell(row, 1),
            nullable: cell(row, 2) == "YES",
            default: row.get(3).cloned().flatten(),
            primary_key: cell(row, 4) == "PRI",
            extra: cell(row, 5),
        });
    }
    columns
}

/// Builds foreign keys from two result sets.
///
/// `keys` has one row per column of a foreign key: schema, table, key id, constraint
/// name, column, parent schema, parent table, parent column. The rows of one key
/// must follow each other, in column order.
/// `unique` has one row per column of a unique index: schema, table, index, column.
fn relations(keys: &ResultSet, unique: &ResultSet) -> Vec<Relation> {
    let mut indexes: HashMap<[String; 3], BTreeSet<String>> = HashMap::new();
    for row in &unique.rows {
        let index = [cell(row, 0), cell(row, 1), cell(row, 2)];
        indexes.entry(index).or_default().insert(cell(row, 3));
    }
    let mut relations: Vec<Relation> = Vec::new();
    let mut current = None;
    for row in &keys.rows {
        let key = [cell(row, 0), cell(row, 1), cell(row, 2)];
        if current.as_ref() != Some(&key) {
            current = Some(key);
            relations.push(Relation {
                name: cell(row, 3),
                schema: row.first().cloned().flatten(),
                table: cell(row, 1),
                columns: Vec::new(),
                ref_schema: row.get(5).cloned().flatten(),
                ref_table: cell(row, 6),
                ref_columns: Vec::new(),
                unique: false,
            });
        }
        if let Some(relation) = relations.last_mut() {
            relation.columns.push(cell(row, 4));
            relation.ref_columns.push(cell(row, 7));
        }
    }
    for relation in &mut relations {
        let schema = relation.schema.clone().unwrap_or_default();
        let columns: BTreeSet<String> = relation.columns.iter().cloned().collect();
        for (index, indexed) in &indexes {
            if index[0] == schema && index[1] == relation.table && *indexed == columns {
                relation.unique = true;
            }
        }
    }
    relations
}

/// Is this table type a view?
fn kind(table_type: &str) -> String {
    let kind = if table_type.to_ascii_lowercase().contains("view") {
        "view"
    } else {
        "table"
    };
    kind.to_owned()
}

/// The statement that sets or drops the default of a column, when it changed.
/// `MySQL` and `PostgreSQL` share this syntax.
fn default_sql(table: &str, column: &str, old: &ColumnInfo, new: &ColumnSpec) -> Option<String> {
    if new.default == old.default {
        return None;
    }
    let action = match &new.default {
        Some(expression) => format!("SET DEFAULT {expression}"),
        None => "DROP DEFAULT".to_owned(),
    };
    Some(format!(
        "ALTER TABLE {table} ALTER COLUMN {column} {action}"
    ))
}

/// Quotes a name the standard SQL way: "name".
fn double_quote(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Quotes a text value the standard SQL way: 'value'.
/// Only safe for `PostgreSQL` and `SQLite`.
// ponytail: a PostgreSQL server with `standard_conforming_strings = off` reads a
// backslash as an escape. That setting has been off by default since 9.1. Write
// `E'...'` strings if such servers must work.
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Cleans the error of a statement that holds a password.
///
/// The message of a syntax error quotes a piece of the SQL, and that piece could
/// be the password. So such a message is replaced. Other messages pass through.
fn password_safe(error: AppError, password: &str) -> AppError {
    let AppError::Database(sqlx::Error::Database(database)) = &error else {
        return error;
    };
    let message = database.message();
    // 42601 is the PostgreSQL code. MySQL has no code for syntax errors alone.
    let syntax_error = database.code().as_deref() == Some("42601")
        || message.starts_with("You have an error in your SQL syntax");
    if syntax_error || message.contains(password) {
        return PASSWORD_ERROR_HIDDEN.into();
    }
    error
}

/// Shows bytes as text when they are valid UTF-8, else as hex.
fn bytes_to_text(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let mut hex = String::with_capacity(2 + bytes.len() * 2);
    hex.push_str("0x");
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
