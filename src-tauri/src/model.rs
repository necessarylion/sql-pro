//! Plain data that moves between the layers and to the frontend.

use std::collections::BTreeMap;

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
