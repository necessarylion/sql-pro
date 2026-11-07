//! Database access. One repository per driver, all behind the same trait.

mod claude;
mod dump;
mod mssql;
mod mysql;
mod password;
mod postgres;
mod sqlite;
mod tunnel;

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use sqlx::pool::{PoolConnection, PoolOptions};
use sqlx::{
    AssertSqlSafe, Column as _, Connection as _, Database, Either, Encode, Executor, IntoArguments,
    Pool, Row as _, Type,
};

pub use claude::ClaudeCli;
pub use dump::DumpRepository;
pub use password::{Keychain, OsKeychain, PasswordRepository};
pub use tunnel::SshTunnel;

use crate::constants::{
    CANCEL_NOT_REACHED, CONNECT_TIMEOUT_SECONDS, DEAD_CONNECTION_SECONDS, LOCALHOST,
    MAX_QUERY_ROWS, NO_PROCESSES, NO_QUERY_STATS, NO_USER_ACCOUNTS, PASSWORD_ERROR_HIDDEN,
    PROCEDURE, TRIGGER, VIEW,
};
use crate::error::{AppError, AppResult};
use crate::model::{
    AccessLevel, ColumnDetail, ColumnInfo, ColumnSpec, ConnectionConfig, DbObject, Driver,
    ForeignKey, IndexInfo, Relation, ResultSet, Row, TableKeys, TableRef, UserList,
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

    /// The character set, collation and comment of each column, for the Structure view.
    /// It is apart from `columns`, so loading a page does not pay for it.
    async fn column_details(&self, _table: &TableRef) -> AppResult<Vec<ColumnDetail>> {
        Ok(Vec::new())
    }

    /// All foreign keys of the current database.
    async fn relations(&self) -> AppResult<Vec<Relation>>;

    /// The indexes of one table. The primary key comes first.
    async fn indexes(&self, table: &TableRef) -> AppResult<Vec<IndexInfo>>;

    /// The foreign keys of one table.
    async fn foreign_keys(&self, table: &TableRef) -> AppResult<Vec<ForeignKey>>;

    /// The indexes, foreign keys and column details of one table, for the Structure view.
    /// This default runs them one after the other. A server overrides it with one batch.
    async fn table_keys(&self, table: &TableRef) -> AppResult<TableKeys> {
        Ok(TableKeys {
            indexes: self.indexes(table).await?,
            foreign_keys: self.foreign_keys(table).await?,
            columns: self.column_details(table).await?,
        })
    }

    /// Fails when this database cannot add or drop a foreign key of an existing table.
    fn check_foreign_key_change(&self) -> AppResult<()> {
        Ok(())
    }

    /// The statement that drops index `name` of `table`. `name` is not quoted yet.
    /// An index lives in the schema of its table, so the schema goes in front.
    fn drop_index_sql(&self, table: &TableRef, name: &str) -> String {
        let index = TableRef {
            schema: table.schema.clone(),
            name: name.to_owned(),
            kind: String::new(),
        };
        format!("DROP INDEX {}", self.table_name(&index))
    }

    /// The statement that drops a foreign key. Both names are already quoted.
    fn drop_foreign_key_sql(&self, table: &str, name: &str) -> String {
        drop_constraint_sql(table, name)
    }

    /// The statement that drops `index` of `table`. An index that a constraint owns
    /// (only `PostgreSQL` and SQL Server tell) is dropped with its constraint.
    fn drop_any_index_sql(&self, table: &TableRef, index: &IndexInfo) -> String {
        if index.constraint {
            return drop_constraint_sql(&self.table_name(table), &self.quote(&index.name));
        }
        self.drop_index_sql(table, &index.name)
    }

    /// The statement that creates `table`, as the database itself writes it.
    /// `None` when the database cannot give one. Then the caller builds it from the columns.
    async fn create_sql(&self, _table: &TableRef) -> AppResult<Option<String>> {
        Ok(None)
    }

    /// The triggers and routines of the current database. Views come with `tables`.
    async fn objects(&self) -> AppResult<Vec<DbObject>>;

    /// The `CREATE` statement of a trigger or routine from `objects`.
    async fn object_sql(&self, object: &DbObject) -> AppResult<String>;

    /// The statement that drops a trigger or routine from `objects`.
    fn drop_object_sql(&self, object: &DbObject) -> String {
        format!("DROP {} {}", keyword(object), self.object_name(object))
    }

    /// The statements that put `sql`, a full `CREATE` statement, in the place of `object`.
    /// The default drops the old object and runs `sql`. The service runs them in one
    /// transaction when `ddl_in_transaction` is true, so a failed `CREATE` keeps the old one.
    fn replace_object_sql(&self, object: &DbObject, sql: &str) -> AppResult<Vec<String>> {
        Ok(vec![self.drop_object_sql(object), sql.to_owned()])
    }

    /// True when `CREATE` and `DROP` can be rolled back inside a transaction.
    fn ddl_in_transaction(&self) -> bool {
        true
    }

    /// The full, quoted name of a trigger or routine.
    fn object_name(&self, object: &DbObject) -> String {
        self.table_name(&TableRef {
            schema: object.schema.clone(),
            name: object.name.clone(),
            kind: String::new(),
        })
    }

    async fn close(&self);

    /// Stops the statement that runs now on the connection of this tab.
    /// The query that runs then fails with the error `QUERY_CANCELLED`.
    /// Does nothing when no statement runs.
    ///
    /// It must not wait for the busy connection of the tab. So `MySQL` and
    /// `PostgreSQL` send the cancel over a second, short-lived connection.
    async fn cancel(&self) -> AppResult<()>;

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

    /// Gives (`on`) or takes away the right to read the server query stats.
    /// The caller has checked that the account has it now only by this right.
    async fn set_user_query_stats(
        &self,
        _name: &str,
        _host: Option<&str>,
        _on: bool,
    ) -> AppResult<()> {
        Err(NO_USER_ACCOUNTS.into())
    }

    /// The SQL that lists the connections of the server. It gives these columns in this
    /// order: id, user, host, database, state, seconds, query, current (1 or 0).
    fn processes_sql(&self) -> AppResult<&'static str> {
        Err(NO_PROCESSES.into())
    }

    /// The SQL that stops process `id`. With `query_only` it stops only the running
    /// statement and the connection stays open.
    fn kill_sql(&self, _id: i64, _query_only: bool) -> AppResult<String> {
        Err(NO_PROCESSES.into())
    }

    /// The SQL for the Query stats view: the statements that ran on the server from all
    /// clients, one row per query shape, the most total time first, at most
    /// `QUERY_STATS_LIMIT` rows. Only shapes that ran in the last `days` days, when the
    /// server keeps a last run time. `days` is a number, so it is safe inside the SQL.
    /// It gives these columns in this order: query, database, runs, last run,
    /// total ms, average ms, max ms, rows.
    fn query_stats_sql(&self, _days: u32) -> AppResult<String> {
        Err(NO_QUERY_STATS.into())
    }

    /// SQL that gives 1 when the server collects query stats. `None` when the stats SQL
    /// fails by itself while they are off.
    fn query_stats_on_sql(&self) -> Option<&'static str> {
        None
    }

    /// The plain text that says how to turn on query stats and which rights they need.
    fn query_stats_setup(&self) -> &'static str {
        NO_QUERY_STATS
    }

    /// The statement that deletes all rows of a table. `table` is already quoted.
    fn truncate_sql(&self, table: &str) -> String {
        format!("TRUNCATE TABLE {table}")
    }

    /// The statement that renames a table. `new_name` is not quoted yet.
    fn rename_table_sql(&self, table: &TableRef, new_name: &str) -> String {
        format!(
            "ALTER TABLE {} RENAME TO {}",
            self.table_name(table),
            self.quote(new_name)
        )
    }

    /// The statement that renames a column. The names are not quoted yet.
    fn rename_column_sql(&self, table: &TableRef, old: &str, new: &str) -> String {
        format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            self.table_name(table),
            self.quote(old),
            self.quote(new)
        )
    }

    /// The statement that adds a column. `table` is quoted, `definition` is the full
    /// column definition.
    fn add_column_sql(&self, table: &str, definition: &str) -> String {
        format!("ALTER TABLE {table} ADD COLUMN {definition}")
    }

    /// The SQL that drops a column. `column` is not quoted yet.
    fn drop_column_sql(&self, table: &TableRef, column: &str) -> String {
        format!(
            "ALTER TABLE {} DROP COLUMN {}",
            self.table_name(table),
            self.quote(column)
        )
    }

    /// Makes the default of each column of `table` a SQL expression, the way `ADD COLUMN`
    /// and `SET DEFAULT` take it. Most databases list it like that already.
    async fn default_expressions(
        &self,
        _table: &TableRef,
        _columns: &mut [ColumnInfo],
    ) -> AppResult<()> {
        Ok(())
    }

    /// SQL that gives the database the connection works on now. A typed `USE` changes it.
    /// `None` when that cannot change (`SQLite`).
    fn current_database_sql(&self) -> Option<&'static str> {
        None
    }

    /// The type that makes its own number sequence, like `serial`, when `column` takes
    /// its default from a sequence that a new table would not have. `None` for most.
    fn serial_type(&self, _column: &ColumnInfo) -> Option<&'static str> {
        None
    }

    /// One column in the select list of a browse or an export.
    fn select_column(&self, column: &ColumnInfo) -> String {
        self.quote(&column.name)
    }

    /// How data compare reads `column`: the select expression, and true when the value it
    /// gives is a SQL literal already, to write as it is. NULL stays NULL.
    fn compare_column(&self, column: &ColumnInfo) -> (String, bool) {
        (self.select_column(column), false)
    }

    /// Per key column name of `table`, what `key_literal` needs to write a value that
    /// compares like the column. Empty when a plain literal does that.
    async fn key_casts(&self, _table: &TableRef) -> AppResult<HashMap<String, KeyCast>> {
        Ok(HashMap::new())
    }

    /// A key value as SQL that compares the way its column sorts in `ORDER BY`.
    /// `cast` is the entry of `key_casts` for the column.
    fn key_literal(&self, _cast: Option<&KeyCast>, value: &str) -> String {
        self.literal(value)
    }

    /// True for a column that only the database writes, so a row write cannot set it.
    fn database_written(&self, _column: &ColumnInfo) -> bool {
        false
    }

    /// The end of a SELECT that reads one page of rows.
    /// `ordering` is empty or ` ORDER BY ...`.
    fn page_sql(&self, ordering: &str, limit: usize, offset: u64) -> String {
        format!("{ordering} LIMIT {limit} OFFSET {offset}")
    }

    /// The SQL around one read of the AI: the start makes the connection read-only,
    /// the end always undoes everything. See `ConnectionService::run_ai_read`.
    /// The default is a read-only transaction that is rolled back.
    fn read_only_guard(&self) -> (&'static str, &'static str) {
        ("START TRANSACTION READ ONLY", "ROLLBACK")
    }

    /// SQL that gives one cell: not `0` when a transaction is open on this connection.
    /// `None` when the database cannot tell, or the guard does not touch transactions.
    fn open_transaction_sql(&self) -> Option<&'static str> {
        None
    }

    /// The words in front of a statement that give its query plan without running it.
    /// `None` when this database has no such words in the app.
    fn explain_prefix(&self) -> Option<&'static str> {
        None
    }

    /// The words between the column list and `VALUES` of an `INSERT` that sets an
    /// identity column by hand. Empty when the database needs none.
    fn identity_override(&self) -> &'static str {
        ""
    }

    /// The statement that lets `INSERT`s set the identity column of `table` by hand
    /// (`on`), or stops that again. `None` when the database needs none.
    fn identity_insert_sql(&self, _table: &TableRef, _on: bool) -> Option<String> {
        None
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

/// The system type and the collation of a key column, from `Repository::key_casts`.
#[derive(Clone, Debug)]
pub struct KeyCast {
    /// The system type with its length, like `varchar(10)`. An alias type gives its base.
    pub data_type: String,
    /// The collation of a text column.
    pub collation: Option<String>,
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
#[derive(Clone, Copy)]
pub struct RepositoryFactory;

impl RepositoryFactory {
    /// Also opens the SSH tunnel when the config asks for one.
    /// `existing` is a tunnel that is open already for this same server. It is used
    /// instead of a new one, because starting `ssh` is the slow part of a connect.
    /// The caller must keep the tunnel alive as long as the repository.
    pub async fn create(
        &self,
        config: &ConnectionConfig,
        existing: Option<Arc<SshTunnel>>,
    ) -> AppResult<(Arc<dyn Repository>, Option<Arc<SshTunnel>>)> {
        let mut tunnel = None;
        let mut tunneled;
        let mut config = config;
        if let (Some(ssh), true) = (&config.ssh, config.driver != Driver::Sqlite) {
            // A tunnel whose ssh died is not used. Then a new one opens.
            let opened = match existing {
                Some(open) if open.is_alive().await => open,
                _ => Arc::new(SshTunnel::open(ssh, &config.host, config.port).await?),
            };
            tunneled = config.clone();
            // The certificate holds the real host name, not the local end of the tunnel.
            tunneled.ssl.server_name.clone_from(&config.host);
            LOCALHOST.clone_into(&mut tunneled.host);
            tunneled.port = opened.local_port();
            tunnel = Some(opened);
            config = &tunneled;
        }
        let repository: Arc<dyn Repository> = match config.driver {
            Driver::Mysql => Arc::new(mysql::MysqlRepository::connect(config).await?),
            Driver::Postgres => Arc::new(postgres::PostgresRepository::connect(config).await?),
            Driver::Sqlite => Arc::new(sqlite::SqliteRepository::connect(config).await?),
            Driver::Mssql => Arc::new(mssql::MssqlRepository::connect(config).await?),
        };
        Ok((repository, tunnel))
    }
}

/// The server id of the one connection of a pool: the thread id in `MySQL`, the
/// process id in `PostgreSQL`. The pool sets it each time it opens a connection,
/// so it stays right after a reconnect. 0 = not known yet.
type ServerId = Arc<AtomicI64>;

/// Opens a new connection with `options`, runs `sql` on it and closes it again.
/// Cancel uses this, because the connection of the tab is busy with the query.
async fn run_on_new_connection<C>(options: &C::Options, sql: String) -> AppResult<()>
where
    C: sqlx::Connection,
    for<'conn> &'conn mut C: Executor<'conn, Database = C::Database>,
{
    let wait = Duration::from_secs(CONNECT_TIMEOUT_SECONDS);
    let Ok(connected) = tokio::time::timeout(wait, C::connect_with(options)).await else {
        return Err(CANCEL_NOT_REACHED.into());
    };
    let mut connection = connected?;
    let result = sqlx::raw_sql(AssertSqlSafe(sql))
        .execute(&mut connection)
        .await;
    // A failed close only leaves a dead socket. The server drops it soon.
    let _ = connection.close().await;
    result?;
    Ok(())
}

/// Pool settings shared by all drivers.
///
/// We keep exactly one connection per tab. This way session state
/// (transactions, `SET`, temp tables) stays the same between queries.
///
/// Reads do not ping the connection first. That saves one round trip per query,
/// which matters on a slow network. When the connection died while idle, one read
/// fails and the next one opens a new connection. Writes ping first (see `live`),
/// so a write never fails only because of an idle dead connection.
// ponytail: one connection per tab, so a slow query blocks that tab. Add a second
// connection for browsing if that hurts.
fn pool_options<DB: Database>() -> PoolOptions<DB> {
    PoolOptions::new()
        .max_connections(1)
        .test_before_acquire(false)
        .idle_timeout(None)
        .max_lifetime(None)
        .acquire_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
}

/// Closes `pool`, but waits at most `DEAD_CONNECTION_SECONDS`.
///
/// `Pool::close` waits until the connection is back in the pool. After hours idle the
/// connection can hang on a dead socket for minutes. Then a connect again (it closes the
/// old pool of the tab) or a disconnect would hang too. The pool is marked closed at
/// once, so a connection that comes back later is closed then.
async fn close_pool<DB: Database>(pool: &Pool<DB>) {
    let wait = Duration::from_secs(DEAD_CONNECTION_SECONDS);
    let _ = tokio::time::timeout(wait, pool.close()).await;
}

/// Takes the one connection of the pool for a write, and checks it first.
///
/// When the ping fails, the dead connection is let go and the pool opens a new one.
/// The pool has only one connection, so the first one is gone before we ask again.
async fn live<DB: Database>(pool: &Pool<DB>) -> AppResult<PoolConnection<DB>> {
    let mut connection = pool.acquire().await?;
    if connection.ping().await.is_ok() {
        return Ok(connection);
    }
    // `detach` frees the place in the pool at once. `close_on_drop` would keep it
    // while it tries to close the dead connection, and the next acquire would wait.
    drop(connection.detach());
    Ok(pool.acquire().await?)
}

/// Runs one statement with bound values on a checked connection.
/// See `Repository::execute`.
async fn execute<DB>(
    pool: &Pool<DB>,
    sql: &str,
    params: Vec<Option<String>>,
    affected: fn(&DB::QueryResult) -> u64,
) -> AppResult<u64>
where
    DB: Database,
    for<'conn> &'conn mut DB::Connection: Executor<'conn, Database = DB>,
    DB::Arguments: IntoArguments<DB>,
    for<'args> Option<String>: Encode<'args, DB> + Type<DB>,
{
    let mut connection = live(pool).await?;
    let mut query = sqlx::query(AssertSqlSafe(sql.to_owned()));
    for param in params {
        query = query.bind(param);
    }
    Ok(affected(&query.execute(&mut *connection).await?))
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
    let mut connection = live(pool).await?;
    let mut transaction = connection.begin().await?;
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
    let mut connection = live(pool).await?;
    let mut transaction = connection.begin().await?;
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

/// Splits the result sets of one `indexes; foreign keys; column details` batch.
fn table_keys(sets: Vec<ResultSet>) -> AppResult<TableKeys> {
    let [index_set, key_set, detail_set] = <[ResultSet; 3]>::try_from(sets).map_err(|sets| {
        AppError::Message(format!(
            "The table keys gave {} result sets instead of 3",
            sets.len()
        ))
    })?;
    Ok(TableKeys {
        indexes: indexes(&index_set),
        foreign_keys: foreign_keys(&key_set),
        columns: details(&detail_set),
    })
}

/// Builds column details from rows shaped as: name, charset, collation, comment.
fn details(set: &ResultSet) -> Vec<ColumnDetail> {
    let mut details = Vec::with_capacity(set.rows.len());
    for row in &set.rows {
        details.push(ColumnDetail {
            name: cell(row, 0),
            charset: row.get(1).cloned().flatten(),
            collation: row.get(2).cloned().flatten(),
            comment: row.get(3).cloned().flatten(),
        });
    }
    details
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

/// Builds indexes from rows shaped as: name, column, unique (YES/NO), primary (YES/NO),
/// and when the database can tell: expression, partial, constraint (all YES/NO) and
/// the full `CREATE INDEX` statement.
/// When a row has a ninth value, the eighth is only the start of the statement, up to the
/// open bracket, and the ninth is the part of this row (`MySQL`). A NULL part means no
/// statement.
/// An empty column also counts as an expression.
/// The rows of one index must follow each other, in column order.
fn indexes(set: &ResultSet) -> Vec<IndexInfo> {
    let mut indexes: Vec<IndexInfo> = Vec::new();
    let mut parts = false;
    for row in &set.rows {
        let name = cell(row, 0);
        let same = indexes.last().is_some_and(|last| last.name == name);
        parts = row.len() > 8;
        if !same {
            indexes.push(IndexInfo {
                name,
                columns: Vec::new(),
                unique: cell(row, 2) == "YES",
                primary: cell(row, 3) == "YES",
                expression: false,
                partial: cell(row, 5) == "YES",
                constraint: cell(row, 6) == "YES",
                definition: row.get(7).cloned().flatten(),
            });
        }
        if let Some(index) = indexes.last_mut() {
            let column = cell(row, 1);
            index.expression |= column.is_empty() || cell(row, 4) == "YES";
            index.columns.push(column);
            if parts {
                match (index.definition.as_mut(), row.get(8).cloned().flatten()) {
                    (Some(definition), Some(part)) => {
                        if same {
                            definition.push_str(", ");
                        }
                        definition.push_str(&part);
                    }
                    _ => index.definition = None,
                }
            }
        }
    }
    if parts {
        for index in &mut indexes {
            if let Some(definition) = &mut index.definition {
                definition.push(')');
            }
        }
    }
    indexes
}

/// The statement that drops a constraint. Both names are already quoted.
/// `PostgreSQL`, SQL Server and `MySQL` 8 share this syntax.
fn drop_constraint_sql(table: &str, name: &str) -> String {
    format!("ALTER TABLE {table} DROP CONSTRAINT {name}")
}

/// Builds foreign keys from rows shaped as: key id, name, column, parent schema,
/// parent table, parent column, on delete, on update.
/// The rows of one key must follow each other, in column order.
fn foreign_keys(set: &ResultSet) -> Vec<ForeignKey> {
    let mut keys: Vec<ForeignKey> = Vec::new();
    let mut current = None;
    for row in &set.rows {
        let id = cell(row, 0);
        if current.as_ref() != Some(&id) {
            current = Some(id);
            keys.push(ForeignKey {
                name: cell(row, 1),
                columns: Vec::new(),
                ref_schema: row.get(3).cloned().flatten(),
                ref_table: cell(row, 4),
                ref_columns: Vec::new(),
                on_delete: cell(row, 6),
                on_update: cell(row, 7),
            });
        }
        if let Some(key) = keys.last_mut() {
            key.columns.push(cell(row, 2));
            key.ref_columns.push(cell(row, 5));
        }
    }
    keys
}

/// Builds triggers and routines from rows shaped as: kind, schema, name, table, args.
fn objects(set: &ResultSet) -> Vec<DbObject> {
    let mut objects = Vec::with_capacity(set.rows.len());
    for row in &set.rows {
        objects.push(DbObject {
            kind: cell(row, 0),
            schema: row.get(1).cloned().flatten(),
            name: cell(row, 2),
            table: row.get(3).cloned().flatten(),
            args: row.get(4).cloned().flatten(),
        });
    }
    objects
}

/// The SQL word for the kind of a trigger or routine.
fn keyword(object: &DbObject) -> &'static str {
    match object.kind.as_str() {
        VIEW => "VIEW",
        TRIGGER => "TRIGGER",
        PROCEDURE => "PROCEDURE",
        _ => "FUNCTION",
    }
}

/// Where the first word of `sql` starts. Comments and spaces in front are skipped.
/// Fails when that word is not `CREATE`.
pub fn create_start(sql: &str) -> AppResult<usize> {
    let mut rest = sql;
    loop {
        rest = rest.trim_start();
        if let Some(comment) = rest.strip_prefix("--") {
            rest = comment.split_once('\n').map_or("", |(_, after)| after);
        } else if let Some(comment) = rest.strip_prefix("/*") {
            rest = comment.split_once("*/").map_or("", |(_, after)| after);
        } else {
            break;
        }
    }
    let word = rest.split(|c: char| !c.is_ascii_alphabetic()).next();
    if word.is_some_and(|word| word.eq_ignore_ascii_case("CREATE")) {
        return Ok(sql.len() - rest.len());
    }
    Err("The SQL must start with CREATE".into())
}

/// Puts `words` in the place of the first `CREATE` of `sql`, like `CREATE OR REPLACE`
/// or `ALTER`. When it says `CREATE OR ...` already, `sql` stays as it is.
pub fn swap_create(sql: &str, words: &str) -> AppResult<String> {
    let start = create_start(sql)?;
    let after = &sql[start + "CREATE".len()..];
    let next = after.split_whitespace().next();
    if next.is_some_and(|word| word.eq_ignore_ascii_case("OR")) {
        return Ok(sql.to_owned());
    }
    Ok(format!("{}{words}{after}", &sql[..start]))
}

/// The first cell of the first row. Fails when it is missing or NULL.
/// A database gives NULL when the account may not read the definition.
fn definition(set: &ResultSet, column: usize, name: &str) -> AppResult<String> {
    match set
        .rows
        .first()
        .and_then(|row| row.get(column).cloned().flatten())
    {
        Some(sql) => Ok(sql),
        None => Err(AppError::Message(format!(
            "Cannot read the definition of {name}. Your account may not be allowed to see it."
        ))),
    }
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
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => hex(bytes),
    }
}

/// Shows bytes as hex, like `0x00ff`.
fn hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(2 + bytes.len() * 2);
    hex.push_str("0x");
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sqlx::sqlite::SqlitePoolOptions;

    use super::{close_pool, create_start, swap_create};
    use crate::constants::DEAD_CONNECTION_SECONDS;

    #[test]
    fn swap_create_keeps_comments_and_or() {
        let sql = "-- note\n/* x */ create view v AS SELECT 1";
        assert_eq!(
            swap_create(sql, "CREATE OR REPLACE").expect("swap"),
            "-- note\n/* x */ CREATE OR REPLACE view v AS SELECT 1"
        );
        assert_eq!(
            swap_create("CREATE PROC p AS SELECT 1", "ALTER").expect("swap"),
            "ALTER PROC p AS SELECT 1"
        );
        let done = "CREATE OR REPLACE FUNCTION f() RETURNS int AS 'SELECT 1'";
        assert_eq!(swap_create(done, "ALTER").expect("swap"), done);
        assert!(swap_create("DROP VIEW v", "ALTER").is_err());
        assert!(create_start("CREATED").is_err());
        assert!(create_start("-- only a comment").is_err());
    }

    /// A connection that never comes back (a hung read) must not make the close hang.
    #[tokio::test]
    async fn close_does_not_wait_for_a_hung_connection() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        let _held = pool.acquire().await.expect("connection");
        let limit = Duration::from_secs(DEAD_CONNECTION_SECONDS + 5);
        tokio::time::timeout(limit, close_pool(&pool))
            .await
            .expect("close hung");
        assert!(pool.is_closed());
    }
}
