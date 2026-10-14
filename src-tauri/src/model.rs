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
}

/// How to reach one database. For `SQLite`, `database` is the file path.
#[derive(Clone, Deserialize)]
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
}

/// An SSH server to tunnel through.
/// `password` is the login password, or the passphrase of the key. Empty means keys and agent only.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub key_path: String,
}

/// A table or a view. Only `PostgreSQL` fills `schema`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableRef {
    pub schema: Option<String>,
    pub name: String,
    #[serde(default)]
    pub kind: String,
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
#[derive(Debug, Default, Serialize)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    pub affected: u64,
    pub truncated: bool,
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
pub struct DbUser {
    pub name: String,
    /// The host part of a `MySQL` account. `None` for `PostgreSQL`.
    pub host: Option<String>,
    pub access: AccessLevel,
    /// True for the account this connection is logged in as.
    pub current: bool,
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
}

/// What to export, and where to write it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub tables: Vec<TableRef>,
    pub format: ExportFormat,
    /// SQL only: write the `CREATE` statement of each table.
    pub structure: bool,
    /// SQL only: write the rows as `INSERT` statements. A CSV file always has the rows.
    pub data: bool,
    /// The columns to export, in this order. Empty means all. Only for one table.
    #[serde(default)]
    pub columns: Vec<String>,
    /// The file to write. For CSV it may be a folder: then each table goes to `<table>.csv` in it.
    pub path: PathBuf,
}

/// Rows the app already shows, to write to a SQL or CSV file.
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
