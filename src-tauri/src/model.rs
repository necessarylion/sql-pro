//! Plain data that moves between the layers and to the frontend.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The database kinds we can talk to.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    Mysql,
    Postgres,
    Sqlite,
    /// Microsoft SQL Server.
    Mssql,
}

/// How to reach one database. For `SQLite`, `database` is the file path.
#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub driver: Driver,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    /// Set to reach the database through an SSH server. Not used for `SQLite`.
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    /// Missing in connections saved before SSL support. Then the default is used,
    /// which connects the same way as before.
    #[serde(default)]
    pub ssl: SslConfig,
}

/// How a connection uses SSL/TLS. Not used for `SQLite`.
/// An empty path means "not set".
#[derive(Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SslConfig {
    pub mode: SslMode,
    /// The CA certificate that signed the server certificate.
    pub ca_path: String,
    /// The client certificate and its key. Not for SQL Server: its driver has no option for them.
    pub cert_path: String,
    pub key_path: String,
    /// The host name the server certificate must hold, when it is not `host`.
    /// The SSH tunnel sets it, because then `host` is `127.0.0.1`. Only SQL Server can use it.
    #[serde(skip)]
    pub server_name: String,
}

/// The SSL modes, like the `sslmode` of `PostgreSQL`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SslMode {
    /// No encryption.
    Off,
    /// Encrypt when the server can. The certificate is not checked. This is the old behaviour.
    #[default]
    Prefer,
    /// Always encrypt. The certificate is not checked.
    Require,
    /// Always encrypt. The certificate must be signed by a trusted CA.
    VerifyCa,
    /// Like `VerifyCa`, and the certificate must also hold the host name.
    VerifyFull,
}

/// An SSH server to tunnel through.
/// `password` is the login password, or the passphrase of the key. Empty means keys and agent only.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub key_path: String,
}

/// A table or a view. Only `PostgreSQL` and SQL Server fill `schema`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableRef {
    pub schema: Option<String>,
    pub name: String,
    #[serde(default)]
    pub kind: String,
}

/// A view, trigger, procedure or function. `kind` is one of those four words.
/// Only `PostgreSQL` and SQL Server fill `schema`. Only `PostgreSQL` fills `args`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbObject {
    pub kind: String,
    pub schema: Option<String>,
    pub name: String,
    /// The table of a trigger.
    pub table: Option<String>,
    /// The argument types of a `PostgreSQL` routine, as the server writes them.
    /// Two functions can have the same name, so the drop needs them.
    pub args: Option<String>,
}

/// One column of a table.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub primary_key: bool,
    pub extra: String,
}

/// One foreign key: `columns` of `table` point at `ref_columns` of `ref_table`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    /// The name of the constraint. Empty when the database gives none.
    pub name: String,
    /// Only `PostgreSQL` fills the two schemas, like in `TableRef`.
    pub schema: Option<String>,
    /// The table that holds the foreign key.
    pub table: String,
    pub columns: Vec<String>,
    pub ref_schema: Option<String>,
    pub ref_table: String,
    /// In the same order as `columns`.
    pub ref_columns: Vec<String>,
    /// True when `columns` are exactly the primary key or one unique index of `table`.
    /// Then one row of `ref_table` has at most one row here.
    pub unique: bool,
}

/// One index of a table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "plain flags from the catalog, each one stands alone"
)]
pub struct IndexInfo {
    /// Empty for the primary key of a `SQLite` row id table. It has no index name.
    pub name: String,
    /// In index order. Empty text for a part that is an expression.
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
    /// True when a part is an expression, like `lower(email)`. Schema compare cannot copy it.
    #[serde(skip)]
    pub expression: bool,
    /// True when the index has a `WHERE` part. Schema compare cannot copy that part.
    #[serde(skip)]
    pub partial: bool,
    /// True when a `UNIQUE` or `PRIMARY KEY` constraint owns the index. Then it is
    /// dropped with `DROP CONSTRAINT`, not with `DROP INDEX`.
    #[serde(skip)]
    pub constraint: bool,
    /// The full `CREATE INDEX` statement, when the database can give it. It keeps the
    /// index kind, the column order, `INCLUDE` columns, prefixes and the `WHERE` part.
    #[serde(skip)]
    pub definition: Option<String>,
}

/// One foreign key of a table: `columns` point at `ref_columns` of `ref_table`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    /// Empty on `SQLite`. It does not keep the names of foreign keys.
    pub name: String,
    pub columns: Vec<String>,
    /// Only set when the referenced table is in another schema or database.
    pub ref_schema: Option<String>,
    pub ref_table: String,
    /// In the same order as `columns`.
    pub ref_columns: Vec<String>,
    /// The rules as the database writes them, like `CASCADE` or `NO ACTION`.
    pub on_delete: String,
    pub on_update: String,
}

/// More about one column, for the Structure view. A database without one of these gives `None`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDetail {
    pub name: String,
    pub charset: Option<String>,
    pub collation: Option<String>,
    pub comment: Option<String>,
}

/// The indexes and foreign keys of one table, and more about its columns.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableKeys {
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKey>,
    /// Empty when the database gives none (`SQLite`).
    pub columns: Vec<ColumnDetail>,
}

/// An index to create.
#[derive(Deserialize)]
pub struct NewIndex {
    pub name: String,
    /// In index order.
    pub columns: Vec<String>,
    pub unique: bool,
}

/// What a foreign key does when the referenced row is deleted or its key changes.
#[derive(Clone, Copy, Debug, Deserialize)]
pub enum ReferentialAction {
    #[serde(rename = "NO ACTION")]
    NoAction,
    #[serde(rename = "RESTRICT")]
    Restrict,
    #[serde(rename = "CASCADE")]
    Cascade,
    #[serde(rename = "SET NULL")]
    SetNull,
    #[serde(rename = "SET DEFAULT")]
    SetDefault,
}

impl ReferentialAction {
    /// The words for SQL, like `SET NULL`.
    pub fn sql(self) -> &'static str {
        match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
        }
    }
}

/// A foreign key to add to a table.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_table: TableRef,
    /// In the same order as `columns`.
    pub ref_columns: Vec<String>,
    pub on_delete: ReferentialAction,
    pub on_update: ReferentialAction,
}

/// One table of the diagram.
#[derive(Debug, Serialize)]
pub struct DiagramTable {
    pub table: TableRef,
    pub columns: Vec<ColumnInfo>,
}

/// The tables of a database and the foreign keys between them.
#[derive(Debug, Serialize)]
pub struct Diagram {
    pub tables: Vec<DiagramTable>,
    pub relations: Vec<Relation>,
}

/// What a new or changed column should look like.
/// `data_type` and `default` are raw SQL, typed by the user.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSpec {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    /// A SQL expression, like `'text'` or `0`. `None` for no default.
    pub default: Option<String>,
}

/// One column of a table that is created.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewColumn {
    #[serde(flatten)]
    pub spec: ColumnSpec,
    pub primary_key: bool,
    /// The database numbers the rows itself: 1, 2, 3.
    /// Only for a column that is the whole primary key.
    pub auto_increment: bool,
}

/// One row. Every value is text, or `None` for NULL.
pub type Row = Vec<Option<String>>;

/// Column name to value. Used for keys, changes and new rows.
pub type Values = BTreeMap<String, Option<String>>;

/// A change of one row. `key` is the primary key of the row, with its values
/// from before the change.
#[derive(Deserialize)]
pub struct RowUpdate {
    pub key: Values,
    pub changes: Values,
}

/// What a saved batch of changes did.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct SaveResult {
    /// The updates that changed a row.
    pub updated: usize,
    pub inserted: usize,
    /// The deletes that removed a row.
    pub deleted: usize,
}

/// The outcome of one SQL statement.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    pub affected: u64,
    pub truncated: bool,
}

/// The choices of the user for one message to Claude. `None` means the default.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiOptions {
    /// Which AI tool runs. One of `AI_PROVIDERS`.
    pub provider: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Claude is told that writes will not run.
    #[serde(default)]
    pub read_only: bool,
}

/// A file for the AI: a PNG, JPEG or WebP image, or a CSV file.
/// `data` is base64 for an image and the text for a CSV file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    /// `image/png`, `image/jpeg`, `image/webp` or `text/csv`.
    pub media_type: String,
    pub data: String,
    /// The number of lines of a CSV file, with the header. 0 for an image.
    #[serde(default)]
    pub rows: usize,
}

/// One reply of Claude in the "Ask Claude" view.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeReply {
    /// Send it with the next message to go on in the same conversation.
    pub session_id: String,
    /// Plain text for the user.
    pub message: String,
    /// One SQL statement Claude wants to run. `None` when Claude is done.
    pub sql: Option<String>,
    /// True when `sql` surely only reads. Else the user must confirm it first.
    pub is_read: bool,
    /// One fact Claude wants to keep in the notes of the database. `None` for nothing.
    pub remember: Option<String>,
    /// Why `remember` was not saved to the notes, like "Notes are full.". `None` when it was.
    pub remember_error: Option<String>,
}

/// The notes about the current database of a tab, for the AI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    /// The safe folder name of the database. The notes window shows it as the title.
    pub database: String,
    /// The full path of the `claude.md` file.
    pub path: String,
    pub text: String,
}

/// One page of table data, with the table's columns.
#[derive(Debug, Serialize)]
pub struct TablePage {
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Row>,
    pub total: u64,
}

/// What page of a table the frontend wants.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseRequest {
    pub table: TableRef,
    /// The body of a WHERE clause, typed by the user. Empty for no filter.
    pub filter: String,
    /// Filter rows from the filter form. All of them must match.
    #[serde(default)]
    pub filters: Vec<FilterRule>,
    /// A column name. Empty to sort by the primary key.
    pub order_by: String,
    pub descending: bool,
    pub limit: usize,
    pub offset: u64,
    /// The total the frontend already knows. When set, the count is skipped.
    /// Only sent when the user moves to another page of the same rows.
    pub total: Option<u64>,
}

/// One row of the filter form: a column, a way to compare, and a value.
#[derive(Deserialize)]
pub struct FilterRule {
    pub column: String,
    pub operator: FilterOperator,
    /// Not used by `IsNull` and `IsNotNull`. For `In` it is a list split by commas.
    pub value: String,
}

/// How a filter rule compares its column with its value.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    In,
    IsNull,
    IsNotNull,
}

/// What an account may do in the database the tab is connected to.
/// `Admin` and `Custom` are only reported. They cannot be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessLevel {
    /// No rights here.
    None,
    /// Can read all tables.
    Read,
    /// Can read all tables and change their rows.
    Write,
    /// Can do everything here.
    Admin,
    /// Anything else, like rights on single tables.
    Custom,
}

/// One login account of the database server.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbUser {
    pub name: String,
    /// The host part of a `MySQL` account. `None` for `PostgreSQL`.
    pub host: Option<String>,
    pub access: AccessLevel,
    /// True for the account this connection is logged in as.
    pub current: bool,
    /// True when the account may read the server query stats (the Query stats view).
    pub query_stats: bool,
    /// True when `query_stats` comes from a wider right, like a superuser.
    /// Then the app cannot change it.
    pub query_stats_fixed: bool,
}

/// The accounts of a server, and whether the logged-in account may manage them.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserList {
    pub can_manage: bool,
    pub users: Vec<DbUser>,
}

/// An account to create. No `Debug` on purpose: it holds a password.
#[derive(Deserialize)]
pub struct NewUser {
    pub name: String,
    pub host: Option<String>,
    pub password: String,
    pub access: AccessLevel,
}

/// The file format of an export.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Sql,
    Csv,
    Json,
    Xlsx,
}

impl ExportFormat {
    /// The file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Sql => "sql",
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Xlsx => "xlsx",
        }
    }
}

/// How far a running backup is. The backend sends it about once per second.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DumpProgress {
    /// The size of the dump file so far.
    pub bytes: u64,
    /// The time since the backup started.
    pub seconds: u64,
    /// The table the dump tool works on now. None before the first table, and for `SQLite`.
    pub table: Option<String>,
    /// How many tables the dump tool has started, this one included.
    pub tables: usize,
}

/// How to back up a `PostgreSQL` database. Other databases ignore it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BackupOptions {
    /// True for the custom format of `pg_dump`, which `pg_restore` reads. Else plain SQL.
    pub custom: bool,
    /// The major version of `pg_dump` to use. None means the default one.
    pub version: Option<u32>,
}

/// How to load a `pg_restore` file into the current database.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RestoreOptions {
    /// The major version of `pg_restore` to use. None means the default one.
    pub version: Option<u32>,
    /// Drop each object before it is made again (`--clean --if-exists`).
    pub clean: bool,
}

/// A `pg_dump` or `pg_restore` that is installed on this computer.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PgTool {
    /// The major version, like 17.
    pub version: u32,
    pub path: PathBuf,
}

/// The versions of one `PostgreSQL` tool on this computer, and the one to use by default.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PgTools {
    /// The major version of the server, like 17.
    pub server: u32,
    /// Newest first.
    pub tools: Vec<PgTool>,
    /// The tool with the version of the server, else the newest one. None when none was found.
    pub default_version: Option<u32>,
}

/// What to export, and where to write it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub tables: Vec<TableRef>,
    pub format: ExportFormat,
    /// SQL only: write the `CREATE` statement of each table.
    pub structure: bool,
    /// SQL only: write the rows as `INSERT` statements. The other formats always have the rows.
    pub data: bool,
    /// The columns to export, in this order. Empty means all. Only for one table.
    #[serde(default)]
    pub columns: Vec<String>,
    /// The file to write. For CSV, JSON and Excel it may be a folder: then each table goes
    /// to its own file in it, like `<table>.csv`. JSON and Excel need a folder for many tables.
    pub path: PathBuf,
}

/// Rows the app already shows, to turn into `INSERT` statements for the clipboard.
#[derive(Debug, Deserialize)]
pub struct RowsSql {
    /// The table that the `INSERT` statements name.
    pub table: TableRef,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

/// Rows the app already shows, to write to a SQL, CSV, JSON or Excel file.
#[derive(Debug, Deserialize)]
pub struct RowsExport {
    pub format: ExportFormat,
    /// SQL only: the table that the `INSERT` statements name.
    pub table: TableRef,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub path: PathBuf,
}

/// One table column that gets its value from a CSV field.
#[derive(Debug, Deserialize)]
pub struct CsvColumn {
    /// The name of the table column.
    pub name: String,
    /// The number of the CSV field, starting at 0.
    pub field: usize,
}

/// What to import from a CSV file, and into which table.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvImport {
    pub table: TableRef,
    pub path: PathBuf,
    /// The first row holds the column names. It is not imported.
    pub header: bool,
    /// An empty field becomes NULL. Else it becomes an empty text.
    pub empty_is_null: bool,
    /// The table columns to fill. Columns that are not here get their default.
    pub columns: Vec<CsvColumn>,
}

/// The start of a CSV file, for the import dialog.
#[derive(Debug, Serialize)]
pub struct CsvPreview {
    /// The first rows, as they are in the file. The header row is one of them.
    pub rows: Vec<Vec<String>>,
    /// All rows of the file, the header row too.
    pub count: usize,
}

/// One connection of the database server, with what it runs now.
#[derive(Debug, Serialize)]
pub struct DbProcess {
    /// `MySQL` connection id, `PostgreSQL` pid or SQL Server session id.
    pub id: i64,
    pub user: Option<String>,
    /// The client host or address.
    pub host: Option<String>,
    pub database: Option<String>,
    pub state: Option<String>,
    /// How long it is in this state, in seconds.
    pub seconds: Option<i64>,
    pub query: Option<String>,
    /// True for the connection of this tab.
    pub current: bool,
}

/// One query shape of the server: all runs of the same statement with other values.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryStat {
    /// The normalized text. Values are replaced, like `WHERE id = ?`.
    pub query: String,
    pub database: Option<String>,
    pub runs: i64,
    /// The time of the last run, as the server writes it. `PostgreSQL` has none.
    pub last_run: Option<String>,
    pub total_ms: f64,
    pub average_ms: f64,
    pub max_ms: Option<f64>,
    /// All rows the runs gave back.
    pub rows: Option<i64>,
}

/// How the output of a plan statement is written, so the service knows how to read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanFormat {
    /// `EXPLAIN (FORMAT JSON)`, with or without `ANALYZE`.
    PostgresJson,
    /// `EXPLAIN FORMAT=JSON` of `MySQL`, and `EXPLAIN` / `ANALYZE FORMAT=JSON` of `MariaDB`.
    /// The two write other fields, but in the same shape.
    MysqlJson,
    /// The text tree of `MySQL` `EXPLAIN ANALYZE`. `MySQL` has no JSON form of it.
    MysqlTree,
    /// The rows of `EXPLAIN QUERY PLAN`: id, parent, unused, detail.
    SqliteRows,
    /// The showplan XML of SQL Server: `SHOWPLAN_XML` or `STATISTICS XML`.
    MssqlXml,
}

/// The SQL that gives the plan of one statement, and how to read what it gives.
#[derive(Debug, PartialEq, Eq)]
pub struct PlanSql {
    /// Runs on its own first, like `SET SHOWPLAN_XML ON`.
    pub before: Option<&'static str>,
    pub sql: String,
    /// Runs on its own at the end, also when `sql` failed. If it fails itself, the
    /// connection is closed, so the setting cannot stay on.
    pub after: Option<&'static str>,
    pub format: PlanFormat,
}

/// A common problem of a plan step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanProblem {
    /// Reads every row of a table.
    FullScan,
    /// Sorts the rows itself, because no index gives the order.
    Filesort,
    /// Writes rows to a temporary table or to disk.
    TempTable,
    /// The real row count is far from the estimate.
    RowEstimate,
}

/// How much of the whole plan one step takes itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanHeat {
    /// `PLAN_SLOW_SHARE` or more.
    Slow,
    /// `PLAN_WARM_SHARE` or more.
    Warm,
}

/// What the shares of a plan are measured in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanMeasure {
    /// Real time, from an analyze.
    Time,
    /// The cost the planner guessed.
    Cost,
}

/// One fact of a plan step, like the filter or the join condition.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanDetail {
    pub name: String,
    pub value: String,
}

/// One step of a query plan, with the steps it reads from. All databases give this shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    /// What the step does, like `Seq Scan` or `Nested loop`.
    pub operation: String,
    /// The table or index it reads, when it reads one.
    pub object: Option<String>,
    pub details: Vec<PlanDetail>,
    /// The rows the planner expects, for one loop.
    pub estimated_rows: Option<f64>,
    /// The real rows, for one loop. Only after an analyze.
    pub actual_rows: Option<f64>,
    /// How often the step ran. Only after an analyze.
    pub loops: Option<f64>,
    /// The cost of the step with its children.
    pub cost: Option<f64>,
    /// The real time of the step with its children, all loops, in milliseconds.
    pub time_ms: Option<f64>,
    /// The part of the whole plan this step takes itself, from 0 to 1.
    pub share: Option<f64>,
    pub heat: Option<PlanHeat>,
    pub problems: Vec<PlanProblem>,
    pub children: Vec<PlanNode>,
    /// The cost of this step alone, when the database gives it. Else it is worked out.
    #[serde(skip)]
    pub own_cost: Option<f64>,
    /// The time of this step alone, when the database gives it. Else it is worked out.
    #[serde(skip)]
    pub own_time: Option<f64>,
}

/// The plan of one statement as a tree, and the plain `EXPLAIN` output.
#[derive(Debug, Serialize)]
pub struct QueryPlan {
    /// The top steps. Most plans have one.
    pub nodes: Vec<PlanNode>,
    /// True when the statement ran, so real times and rows are there.
    pub analyzed: bool,
    /// What `share` is measured in. `None` when the plan has no times and no costs.
    pub measure: Option<PlanMeasure>,
    /// The time (ms) or cost of the whole plan.
    pub total: Option<f64>,
    /// The output of the plain `EXPLAIN`, for the Raw tab.
    pub raw: Option<ResultSet>,
}

#[cfg(test)]
mod tests {
    use super::{ConnectionConfig, SslMode};

    #[test]
    fn a_connection_saved_before_ssl_support_still_loads() {
        let json = r#"{"driver":"mysql","host":"h","port":3306,"user":"u","password":"","database":"","ssh":null}"#;
        let config: ConnectionConfig = serde_json::from_str(json).expect("config");
        assert_eq!(config.ssl.mode, SslMode::Prefer);
        let json = r#"{"driver":"mysql","host":"h","port":3306,"user":"u","password":"","database":"","ssl":{"mode":"verifyCa","caPath":"/ca.pem"}}"#;
        let config: ConnectionConfig = serde_json::from_str(json).expect("config");
        assert_eq!(config.ssl.mode, SslMode::VerifyCa);
        assert_eq!(config.ssl.ca_path, "/ca.pem");
    }
}

/// What one difference of a schema compare does to the target database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeAction {
    /// The table is only in the source.
    CreateTable,
    /// The table is only in the target.
    DropTable,
    AddColumn,
    DropColumn,
    /// The type, null rule or default of the column differs.
    AlterColumn,
    AddIndex,
    DropIndex,
    /// The columns or the unique rule of the index differ. It is dropped and made again.
    AlterIndex,
    AddForeignKey,
    DropForeignKey,
}

/// One difference between the source and the target of a schema compare.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaChange {
    /// Picks this difference when the sync SQL is made.
    pub id: String,
    /// The table in the target. For `CreateTable` the table of the source.
    pub table: TableRef,
    pub action: ChangeAction,
    /// The column, index or foreign key. Empty for a table, and for a foreign key without a name.
    pub name: String,
    /// What differs, like `int NOT NULL → bigint`.
    pub detail: String,
}

/// The SQL that makes the target like the source, in a safe order.
#[derive(Debug, Default, Serialize)]
pub struct SyncSql {
    pub statements: Vec<String>,
    /// The picked differences that cannot be done in SQL here, with the reason.
    pub warnings: Vec<String>,
    /// The database the target tab worked on when the SQL was made. `None` on `SQLite`.
    /// The run sends it back, so SQL made for one database never runs on another.
    pub database: Option<String>,
}

/// The kinds of row changes that go into the data sync SQL.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct DataKinds {
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
}

/// How the rows of one table differ between two databases, and the SQL that makes the
/// target rows the same as the source rows.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDiff {
    /// The primary key columns. The rows are matched by them.
    pub key: Vec<String>,
    /// The columns both sides have. Only these are compared and written.
    pub columns: Vec<String>,
    pub source_only: Vec<String>,
    pub target_only: Vec<String>,
    /// Columns the database fills itself: generated, identity outside the key, and SQL
    /// Server `rowversion`. A row write cannot set them, so they are left out.
    pub generated: Vec<String>,
    /// True when there are more than `MAX_DATA_DIFFS` differences. Then there is no SQL.
    pub limited: bool,
    /// With `limited`: too many rows waited for a match, but few are surely different.
    /// The two databases likely sort the key in another order (another collation).
    pub unsorted: bool,
    /// The rows that change. `inserts` can also hold the statements around the inserts.
    pub delete_rows: usize,
    pub update_rows: usize,
    pub insert_rows: usize,
    pub deletes: Vec<String>,
    pub updates: Vec<String>,
    pub inserts: Vec<String>,
    /// The database the target tab worked on at the compare. `None` on `SQLite`.
    pub database: Option<String>,
}
