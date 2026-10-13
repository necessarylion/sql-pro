//! Business logic: open connections and the SQL for browsing and changing rows.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::constants::{LIKE_ESCAPE, LOCALHOST, MAX_QUERY_ROWS};
use crate::error::{AppError, AppResult};
use crate::model::{
    BrowseRequest, ColumnInfo, ColumnSpec, ConnectionConfig, FilterOperator, FilterRule, ResultSet,
    TablePage, TableRef, Values,
};
use crate::repository::{DumpRepository, Repository, RepositoryFactory, SshTunnel};

/// The `kind` of a `TableRef` that is a view.
const VIEW: &str = "view";

/// One open connection. Each tab of the app owns one.
#[derive(Clone)]
struct Connection {
    config: ConnectionConfig,
    repository: Arc<dyn Repository>,
    /// Lives as long as the connection. It also knows the local port of the tunnel.
    tunnel: Option<Arc<SshTunnel>>,
}

/// Keeps the open connections and does all the work on them.
pub struct ConnectionService {
    factory: RepositoryFactory,
    connections: Mutex<HashMap<String, Connection>>,
}

impl ConnectionService {
    pub fn new(factory: RepositoryFactory) -> Self {
        Self {
            factory,
            connections: Mutex::new(HashMap::new()),
        }
    }

    /// Opens a connection under `id`. An older connection with the same id is closed.
    pub async fn connect(&self, id: String, config: ConnectionConfig) -> AppResult<()> {
        let (repository, tunnel) = self.factory.create(&config).await?;
        let connection = Connection {
            config,
            repository,
            tunnel: tunnel.map(Arc::new),
        };
        let previous = self.connections.lock().insert(id, connection);
        if let Some(previous) = previous {
            previous.repository.close().await;
        }
        Ok(())
    }

    pub async fn disconnect(&self, id: &str) {
        let removed = self.connections.lock().remove(id);
        if let Some(removed) = removed {
            removed.repository.close().await;
        }
    }

    /// Switches to another database. `PostgreSQL` needs a new connection for this,
    /// so we reconnect for every driver.
    pub async fn use_database(&self, id: &str, database: String) -> AppResult<()> {
        let mut config = self.connection(id)?.config;
        config.database = database;
        self.connect(id.to_owned(), config).await
    }

    pub async fn databases(&self, id: &str) -> AppResult<Vec<String>> {
        self.repository(id)?.databases().await
    }

    pub async fn tables(&self, id: &str) -> AppResult<Vec<TableRef>> {
        self.repository(id)?.tables().await
    }

    /// Runs SQL typed by the user.
    pub async fn run_query(&self, id: &str, sql: &str) -> AppResult<Vec<ResultSet>> {
        self.repository(id)?.query(sql).await
    }

    /// Writes a full SQL dump of the current database to the file at `path`.
    ///
    /// The dump tool is its own process with its own login. So the tab can be
    /// used while it runs, and no lock is held.
    pub async fn dump_database(&self, id: &str, path: &Path) -> AppResult<()> {
        // This copy also keeps the tunnel open until the dump is done.
        let connection = self.connection(id)?;
        let mut config = connection.config;
        if let Some(tunnel) = &connection.tunnel {
            // Go through the same tunnel as the tab.
            LOCALHOST.clone_into(&mut config.host);
            config.port = tunnel.local_port();
        }
        DumpRepository::dump(&config, path).await
    }

    /// Loads one page of a table, with its columns and the total row count.
    // ponytail: COUNT(*) on every page load. It is slow on very large tables;
    // use the engine's row estimate when that hurts.
    pub async fn browse(&self, id: &str, request: &BrowseRequest) -> AppResult<TablePage> {
        let repository = self.repository(id)?;
        let columns = repository.columns(&request.table).await?;
        let table = repository.table_name(&request.table);

        let condition = Self::where_clause(repository.as_ref(), &columns, request)?;

        let mut order = Vec::new();
        if request.order_by.is_empty() {
            // Sort by the primary key, so rows do not jump around after an edit.
            for column in &columns {
                if column.primary_key {
                    order.push(repository.quote(&column.name));
                }
            }
        } else {
            let column = Self::column(&columns, &request.order_by)?;
            let direction = if request.descending { " DESC" } else { "" };
            order.push(format!("{}{direction}", repository.quote(&column.name)));
        }
        let ordering = if order.is_empty() {
            String::new()
        } else {
            format!(" ORDER BY {}", order.join(", "))
        };

        let limit = request.limit.min(MAX_QUERY_ROWS);
        let offset = request.offset;
        let rows_sql =
            format!("SELECT * FROM {table}{condition}{ordering} LIMIT {limit} OFFSET {offset}");
        let count_sql = format!("SELECT COUNT(*) FROM {table}{condition}");

        let rows = repository
            .query(&rows_sql)
            .await?
            .pop()
            .unwrap_or_default()
            .rows;
        let count = repository
            .query(&count_sql)
            .await?
            .pop()
            .unwrap_or_default();
        let total = match count.rows.first().and_then(|row| row.first()) {
            Some(Some(text)) => text.parse().unwrap_or(0),
            _ => 0,
        };
        Ok(TablePage {
            columns,
            rows,
            total,
        })
    }

    /// Changes columns of the one row that `key` points at.
    pub async fn update_row(
        &self,
        id: &str,
        table: &TableRef,
        key: Values,
        changes: Values,
    ) -> AppResult<u64> {
        if changes.is_empty() {
            return Ok(0);
        }
        let repository = self.repository(id)?;
        let columns = repository.columns(table).await?;
        let mut params = Vec::new();
        let mut assignments = Vec::with_capacity(changes.len());
        for (name, value) in changes {
            let column = Self::column(&columns, &name)?;
            params.push(value);
            assignments.push(Self::equals(repository.as_ref(), column, params.len()));
        }
        let condition = Self::key_condition(repository.as_ref(), &columns, key, &mut params)?;
        let sql = format!(
            "UPDATE {} SET {} WHERE {condition}",
            repository.table_name(table),
            assignments.join(", ")
        );
        repository.execute(&sql, params).await
    }

    /// Adds one row. Columns that are not in `values` get their default.
    pub async fn insert_row(&self, id: &str, table: &TableRef, values: Values) -> AppResult<u64> {
        let repository = self.repository(id)?;
        let columns = repository.columns(table).await?;
        let mut params = Vec::new();
        let mut names = Vec::with_capacity(values.len());
        let mut placeholders = Vec::with_capacity(values.len());
        for (name, value) in values {
            let column = Self::column(&columns, &name)?;
            params.push(value);
            names.push(repository.quote(&column.name));
            placeholders.push(repository.placeholder(params.len(), &column.data_type));
        }
        let table = repository.table_name(table);
        let sql = if names.is_empty() {
            format!("INSERT INTO {table} {}", repository.default_values())
        } else {
            format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                names.join(", "),
                placeholders.join(", ")
            )
        };
        repository.execute(&sql, params).await
    }

    /// Deletes the rows that `keys` point at. Gives the number of deleted rows.
    pub async fn delete_rows(
        &self,
        id: &str,
        table: &TableRef,
        keys: Vec<Values>,
    ) -> AppResult<u64> {
        let repository = self.repository(id)?;
        let columns = repository.columns(table).await?;
        let table = repository.table_name(table);
        let mut deleted = 0;
        for key in keys {
            let mut params = Vec::new();
            let condition = Self::key_condition(repository.as_ref(), &columns, key, &mut params)?;
            let sql = format!("DELETE FROM {table} WHERE {condition}");
            deleted += repository.execute(&sql, params).await?;
        }
        Ok(deleted)
    }

    /// Adds a column to a table.
    pub async fn add_column(
        &self,
        id: &str,
        table: &TableRef,
        column: &ColumnSpec,
    ) -> AppResult<()> {
        Self::check_spec(column)?;
        let repository = self.repository(id)?;
        let mut definition = format!("{} {}", repository.quote(&column.name), column.data_type);
        if !column.nullable {
            definition.push_str(" NOT NULL");
        }
        if let Some(default) = &column.default {
            definition.push_str(" DEFAULT ");
            definition.push_str(default);
        }
        let sql = format!(
            "ALTER TABLE {} ADD COLUMN {definition}",
            repository.table_name(table)
        );
        repository.query(&sql).await?;
        Ok(())
    }

    /// Removes a column and all its data.
    pub async fn drop_column(&self, id: &str, table: &TableRef, name: &str) -> AppResult<()> {
        let repository = self.repository(id)?;
        let columns = repository.columns(table).await?;
        let column = Self::column(&columns, name)?;
        let sql = format!(
            "ALTER TABLE {} DROP COLUMN {}",
            repository.table_name(table),
            repository.quote(&column.name)
        );
        repository.query(&sql).await?;
        Ok(())
    }

    /// Changes column `name` so it matches `column`. Only the parts that differ are touched.
    // ponytail: the statements run one by one, not in a transaction (MySQL cannot
    // roll back DDL anyway). If one fails, the earlier ones stay.
    // ponytail: RENAME COLUMN needs MySQL 8, MariaDB 10.5 or SQLite 3.25.
    pub async fn alter_column(
        &self,
        id: &str,
        table: &TableRef,
        name: &str,
        column: &ColumnSpec,
    ) -> AppResult<()> {
        Self::check_spec(column)?;
        let repository = self.repository(id)?;
        let columns = repository.columns(table).await?;
        let old = Self::column(&columns, name)?;
        let mut statements = repository.modify_column_sql(table, old, column).await?;
        if column.name != old.name {
            statements.push(format!(
                "ALTER TABLE {} RENAME COLUMN {} TO {}",
                repository.table_name(table),
                repository.quote(&old.name),
                repository.quote(&column.name)
            ));
        }
        for statement in statements {
            repository.query(&statement).await?;
        }
        Ok(())
    }

    /// Removes a table and all its data, or a view.
    // No CASCADE on purpose. When other objects need the table, the database says no.
    pub async fn drop_table(&self, id: &str, table: &TableRef) -> AppResult<()> {
        let repository = self.repository(id)?;
        let table = Self::known_table(repository.as_ref(), table).await?;
        let keyword = if table.kind == VIEW { "VIEW" } else { "TABLE" };
        let sql = format!("DROP {keyword} {}", repository.table_name(&table));
        repository.query(&sql).await?;
        Ok(())
    }

    /// Deletes all rows of a table. The table itself stays.
    pub async fn truncate_table(&self, id: &str, table: &TableRef) -> AppResult<()> {
        let repository = self.repository(id)?;
        let table = Self::known_table(repository.as_ref(), table).await?;
        if table.kind == VIEW {
            return Err("A view has no rows of its own, so it cannot be truncated".into());
        }
        let sql = repository.truncate_sql(&repository.table_name(&table));
        repository.query(&sql).await?;
        Ok(())
    }

    /// Finds a table in the current database. Stops names that are not in it.
    /// Gives the table as the database lists it, so `kind` is the real one.
    async fn known_table(repository: &dyn Repository, table: &TableRef) -> AppResult<TableRef> {
        for known in repository.tables().await? {
            if known.name == table.name && known.schema == table.schema {
                return Ok(known);
            }
        }
        Err(AppError::Message(format!("Unknown table: {}", table.name)))
    }

    fn check_spec(column: &ColumnSpec) -> AppResult<()> {
        if column.name.trim().is_empty() {
            return Err("The column needs a name".into());
        }
        if column.data_type.trim().is_empty() {
            return Err("The column needs a type".into());
        }
        Ok(())
    }

    fn connection(&self, id: &str) -> AppResult<Connection> {
        Ok(self
            .connections
            .lock()
            .get(id)
            .cloned()
            .ok_or("This tab is not connected")?)
    }

    fn repository(&self, id: &str) -> AppResult<Arc<dyn Repository>> {
        Ok(self.connection(id)?.repository)
    }

    /// Finds a column by name. Stops names that are not part of the table.
    fn column<'cols>(columns: &'cols [ColumnInfo], name: &str) -> AppResult<&'cols ColumnInfo> {
        for column in columns {
            if column.name == name {
                return Ok(column);
            }
        }
        Err(AppError::Message(format!("Unknown column: {name}")))
    }

    /// Builds the WHERE part of a browse: all filter rules and the raw filter, joined with AND.
    /// Gives an empty string when there is nothing to filter on.
    fn where_clause(
        repository: &dyn Repository,
        columns: &[ColumnInfo],
        request: &BrowseRequest,
    ) -> AppResult<String> {
        let mut conditions = Vec::with_capacity(request.filters.len() + 1);
        for rule in &request.filters {
            conditions.push(Self::rule_condition(repository, columns, rule)?);
        }
        let filter = request.filter.trim();
        if conditions.is_empty() {
            // The raw filter alone goes in as typed. So a tail like `1 ORDER BY x` still works.
            conditions.push(filter.to_owned());
        } else if !filter.is_empty() {
            conditions.push(format!("({filter})"));
        }
        let joined = conditions.join(" AND ");
        if joined.is_empty() {
            return Ok(String::new());
        }
        Ok(format!(" WHERE {joined}"))
    }

    /// Builds the SQL condition of one filter rule.
    ///
    /// Browsing uses the text protocol, so values cannot be bound. Every value
    /// goes through `Repository::literal` instead. Never put a value in raw.
    fn rule_condition(
        repository: &dyn Repository,
        columns: &[ColumnInfo],
        rule: &FilterRule,
    ) -> AppResult<String> {
        let column = repository.quote(&Self::column(columns, &rule.column)?.name);
        let value = &rule.value;
        let sign = match rule.operator {
            FilterOperator::Eq => "=",
            FilterOperator::Neq => "<>",
            FilterOperator::Lt => "<",
            FilterOperator::Gt => ">",
            FilterOperator::Lte => "<=",
            FilterOperator::Gte => ">=",
            FilterOperator::Contains => {
                return Ok(Self::like(repository, &column, "LIKE", "%", value, "%"));
            }
            FilterOperator::NotContains => {
                return Ok(Self::like(repository, &column, "NOT LIKE", "%", value, "%"));
            }
            FilterOperator::StartsWith => {
                return Ok(Self::like(repository, &column, "LIKE", "", value, "%"));
            }
            FilterOperator::EndsWith => {
                return Ok(Self::like(repository, &column, "LIKE", "%", value, ""));
            }
            FilterOperator::In => return Self::in_list(repository, &column, value),
            FilterOperator::IsNull => return Ok(format!("{column} IS NULL")),
            FilterOperator::IsNotNull => return Ok(format!("{column} IS NOT NULL")),
        };
        Ok(format!("{column} {sign} {}", repository.literal(value)))
    }

    /// Builds `column LIKE 'pattern'`. The value is matched as plain text:
    /// `%` and `_` typed by the user are not wildcards.
    fn like(
        repository: &dyn Repository,
        column: &str,
        keyword: &str,
        before: &str,
        value: &str,
        after: &str,
    ) -> String {
        let mut pattern = String::from(before);
        for character in value.chars() {
            if matches!(character, '%' | '_' | LIKE_ESCAPE) {
                pattern.push(LIKE_ESCAPE);
            }
            pattern.push(character);
        }
        pattern.push_str(after);
        format!(
            "{} {keyword} {} ESCAPE '{LIKE_ESCAPE}'",
            repository.like_operand(column),
            repository.literal(&pattern)
        )
    }

    /// Builds `column IN ('a', 'b')` from a list split by commas.
    // ponytail: a value that holds a comma cannot be in the list. Take a real
    // list from the frontend if that is needed.
    fn in_list(repository: &dyn Repository, column: &str, value: &str) -> AppResult<String> {
        let mut items = Vec::new();
        for part in value.split(',') {
            let part = part.trim();
            if !part.is_empty() {
                items.push(repository.literal(part));
            }
        }
        if items.is_empty() {
            return Err("The IN filter needs at least one value".into());
        }
        Ok(format!("{column} IN ({})", items.join(", ")))
    }

    /// Builds `column = value marker` for bound value number `index`.
    fn equals(repository: &dyn Repository, column: &ColumnInfo, index: usize) -> String {
        format!(
            "{} = {}",
            repository.quote(&column.name),
            repository.placeholder(index, &column.data_type)
        )
    }

    /// Builds the WHERE part that matches exactly one row by its full primary key.
    ///
    /// It fails when the table has no primary key or a key value is missing.
    /// This is the guard that stops an UPDATE or DELETE from hitting many rows.
    // ponytail: tables without a primary key are read only. Match on all column
    // values (like Sequel Pro) if editing them is needed.
    fn key_condition(
        repository: &dyn Repository,
        columns: &[ColumnInfo],
        mut key: Values,
        params: &mut Vec<Option<String>>,
    ) -> AppResult<String> {
        let mut parts = Vec::new();
        for column in columns {
            if !column.primary_key {
                continue;
            }
            let value = key.remove(&column.name).flatten();
            let value = value.ok_or("The row has no value for its primary key")?;
            params.push(Some(value));
            parts.push(Self::equals(repository, column, params.len()));
        }
        if parts.is_empty() {
            return Err("This table has no primary key, so its rows cannot be changed".into());
        }
        Ok(parts.join(" AND "))
    }
}
