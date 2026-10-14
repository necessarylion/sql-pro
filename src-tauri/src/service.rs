//! Business logic: open connections and the SQL for browsing and changing rows.

mod export;
mod import;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::constants::{
    ANY_HOST, CANNOT_MANAGE_USERS, LIKE_ESCAPE, LOCALHOST, MAX_QUERY_ROWS, NO_USER_ACCOUNTS,
    PRIMARY_KEY_NOT_DROPPED, VIEW,
};
use crate::error::{AppError, AppResult};
use crate::model::{
    AccessLevel, BrowseRequest, ColumnInfo, ColumnSpec, ConnectionConfig, DbObject, DbUser,
    Diagram, DiagramTable, Driver, ExportRequest, FilterOperator, FilterRule, NewColumn,
    NewForeignKey, NewIndex, NewUser, ResultSet, RowUpdate, RowsExport, SaveResult, TableKeys,
    TablePage, TableRef, UserList, Values,
};
use crate::repository::{
    DumpRepository, Repository, RepositoryFactory, SshTunnel, Statement, not_saved,
};

/// The bound values of one statement.
type Params = Vec<Option<String>>;

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

    /// Stops the query that runs now in tab `id`.
    ///
    /// The lock on the connection list is held only to copy the repository out.
    /// So this never waits for the running query.
    pub async fn cancel_query(&self, id: &str) -> AppResult<()> {
        self.repository(id)?.cancel().await
    }

    /// The tables of the current database with their columns, and the foreign keys
    /// between them. Views are left out.
    // ponytail: one columns query per table. Load all columns in one query per
    // driver if a database with very many tables opens too slowly.
    pub async fn diagram(&self, id: &str) -> AppResult<Diagram> {
        let repository = self.repository(id)?;
        let mut tables = Vec::new();
        for table in repository.tables().await? {
            if table.kind != VIEW {
                let columns = repository.columns(&table).await?;
                tables.push(DiagramTable { table, columns });
            }
        }
        let relations = repository.relations().await?;
        Ok(Diagram { tables, relations })
    }

    /// Writes a picture of the diagram to the file at `path`.
    /// An older file at `path` is replaced.
    pub fn save_image(path: &Path, data: &[u8]) -> AppResult<()> {
        Ok(std::fs::write(path, data)?)
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

    /// Writes tables to a SQL or CSV file. See `ExportRequest`.
    /// It only reads, on the connection of the tab. So the tab waits until it is done.
    pub async fn export_tables(&self, id: &str, request: &ExportRequest) -> AppResult<()> {
        let repository = self.repository(id)?;
        export::Exporter::new(repository.as_ref(), request)
            .run()
            .await
    }

    /// Writes rows the app already has to a SQL or CSV file. It does not touch the database.
    pub fn export_rows(&self, id: &str, request: &RowsExport) -> AppResult<()> {
        let repository = self.repository(id)?;
        export::Exporter::write_rows(repository.as_ref(), request)
    }

    /// The columns of a table or view.
    pub async fn columns(&self, id: &str, table: &TableRef) -> AppResult<Vec<ColumnInfo>> {
        self.repository(id)?.columns(table).await
    }

    /// Loads one page of a table, with its columns and the total row count.
    ///
    /// When the request brings a known total (the user only moves to another page),
    /// the count is skipped and that total comes back as it is.
    // ponytail: COUNT(*) on every other load. It is slow on very large tables;
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
        if let Some(total) = request.total {
            let rows = repository
                .query(&rows_sql)
                .await?
                .pop()
                .unwrap_or_default()
                .rows;
            return Ok(TablePage {
                columns,
                rows,
                total,
            });
        }

        // Rows and count go in one round trip. Every statement gives its own result
        // set, also a SELECT with no rows, so the count is always the last set.
        // The newline ends a `--` comment the user may have typed in the filter.
        let sql = format!("{rows_sql}\n; SELECT COUNT(*) FROM {table}{condition}");
        let mut sets = repository.query(&sql).await?;
        let count = sets.pop().unwrap_or_default();
        let rows = sets.into_iter().next().unwrap_or_default().rows;
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
        let (sql, params) = Self::update_sql(repository.as_ref(), table, &columns, key, changes)?;
        repository.execute(&sql, params).await
    }

    /// Adds one row. Columns that are not in `values` get their default.
    pub async fn insert_row(&self, id: &str, table: &TableRef, values: Values) -> AppResult<u64> {
        let repository = self.repository(id)?;
        let columns = repository.columns(table).await?;
        let (sql, params) = Self::insert_sql(repository.as_ref(), table, &columns, values)?;
        repository.execute(&sql, params).await
    }

    /// Writes many row changes at once: first the deletes, then the updates, then the new rows.
    /// The deletes go first, so a new or changed row can take the key of a deleted row.
    /// All or nothing. When one change fails, none of them is kept.
    /// An update with no changes is skipped. `deletes` holds the keys of the rows to delete.
    pub async fn save_changes(
        &self,
        id: &str,
        table: &TableRef,
        updates: Vec<RowUpdate>,
        inserts: Vec<Values>,
        deletes: Vec<Values>,
    ) -> AppResult<SaveResult> {
        let repository = self.repository(id)?;
        if updates.is_empty() && inserts.is_empty() && deletes.is_empty() {
            return Ok(SaveResult::default());
        }
        let columns = repository.columns(table).await?;
        let mut statements = Vec::with_capacity(deletes.len() + updates.len() + inserts.len());
        for (index, key) in deletes.into_iter().enumerate() {
            let label = format!("Deleted row {}", index + 1);
            let built = Self::delete_sql(repository.as_ref(), table, &columns, key);
            statements.push(Self::statement(label, built)?);
        }
        let delete_count = statements.len();
        for (index, update) in updates.into_iter().enumerate() {
            if update.changes.is_empty() {
                continue;
            }
            let label = format!("Row {}", index + 1);
            let RowUpdate { key, changes } = update;
            let built = Self::update_sql(repository.as_ref(), table, &columns, key, changes);
            statements.push(Self::statement(label, built)?);
        }
        let update_end = statements.len();
        for (index, values) in inserts.into_iter().enumerate() {
            let label = format!("New row {}", index + 1);
            let built = Self::insert_sql(repository.as_ref(), table, &columns, values);
            statements.push(Self::statement(label, built)?);
        }
        if statements.is_empty() {
            return Ok(SaveResult::default());
        }
        let counts = repository.execute_all(statements).await?;
        Ok(SaveResult {
            updated: Self::changed(&counts[delete_count..update_end]),
            inserted: counts.len() - update_end,
            deleted: Self::changed(&counts[..delete_count]),
        })
    }

    /// Opens a second connection under `new_id`, with the settings that `id` uses now.
    /// It is on its own: it has its own SSH tunnel, and closing one leaves the other open.
    pub async fn clone_connection(&self, id: &str, new_id: String) -> AppResult<()> {
        let config = self.connection(id)?.config;
        self.connect(new_id, config).await
    }

    /// Adds a column to a table.
    pub async fn add_column(
        &self,
        id: &str,
        table: &TableRef,
        column: &ColumnSpec,
    ) -> AppResult<()> {
        let repository = self.repository(id)?;
        let sql = format!(
            "ALTER TABLE {} ADD COLUMN {}",
            repository.table_name(table),
            Self::column_definition(repository.as_ref(), column, column.nullable)?
        );
        repository.query(&sql).await?;
        Ok(())
    }

    /// Creates a table with one `CREATE TABLE` statement.
    // ponytail: the table goes to the default schema of the connection. In
    // PostgreSQL that is the first schema of the search path, mostly `public`.
    // Take a schema here if tables must be created in other schemas.
    pub async fn create_table(&self, id: &str, name: &str, columns: &[NewColumn]) -> AppResult<()> {
        Self::check_table_name(name)?;
        if columns.is_empty() {
            return Err("The table needs at least one column".into());
        }
        let repository = self.repository(id)?;
        let mut keys = Vec::new();
        for column in columns {
            if column.primary_key {
                keys.push(repository.quote(&column.spec.name));
            }
        }
        let mut parts = Vec::with_capacity(columns.len() + 1);
        for column in columns {
            // A primary key column never holds NULL. SQLite would allow it without this.
            let nullable = column.spec.nullable && !column.primary_key;
            let mut definition =
                Self::column_definition(repository.as_ref(), &column.spec, nullable)?;
            if column.primary_key && keys.len() == 1 {
                definition.push_str(" PRIMARY KEY");
                if column.auto_increment {
                    definition.push(' ');
                    definition.push_str(repository.auto_increment());
                }
            } else if column.auto_increment {
                return Err(
                    "Auto increment only works on a column that is the whole primary key".into(),
                );
            }
            parts.push(definition);
        }
        if keys.len() > 1 {
            parts.push(format!("PRIMARY KEY ({})", keys.join(", ")));
        }
        let sql = format!(
            "CREATE TABLE {} ({})",
            repository.quote(name),
            parts.join(", ")
        );
        repository.query(&sql).await?;
        Ok(())
    }

    /// Gives a table a new name. It stays in its schema.
    pub async fn rename_table(&self, id: &str, table: &TableRef, name: &str) -> AppResult<()> {
        Self::check_table_name(name)?;
        let repository = self.repository(id)?;
        let table = Self::known_table(repository.as_ref(), table).await?;
        if table.kind == VIEW {
            return Err("Only a table can be renamed here, not a view".into());
        }
        let sql =
            repository.rename_table_sql(&repository.table_name(&table), &repository.quote(name));
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

    /// The indexes and foreign keys of a table.
    pub async fn table_keys(&self, id: &str, table: &TableRef) -> AppResult<TableKeys> {
        let repository = self.repository(id)?;
        Ok(TableKeys {
            indexes: repository.indexes(table).await?,
            foreign_keys: repository.foreign_keys(table).await?,
        })
    }

    /// Creates an index on columns of the table. All three databases share this syntax.
    pub async fn add_index(&self, id: &str, table: &TableRef, index: &NewIndex) -> AppResult<()> {
        let name = Self::key_name("index", &index.name)?;
        let repository = self.repository(id)?;
        let columns = repository.columns(table).await?;
        let sql = format!(
            "CREATE {}INDEX {} ON {} ({})",
            if index.unique { "UNIQUE " } else { "" },
            repository.quote(name),
            repository.table_name(table),
            Self::column_list(repository.as_ref(), &columns, &index.columns)?
        );
        repository.query(&sql).await?;
        Ok(())
    }

    /// Drops an index of the table. The primary key is refused.
    pub async fn drop_index(&self, id: &str, table: &TableRef, name: &str) -> AppResult<()> {
        let repository = self.repository(id)?;
        let indexes = repository.indexes(table).await?;
        let Some(index) = indexes.iter().find(|index| index.name == name) else {
            return Err(AppError::Message(format!("Unknown index: {name}")));
        };
        if index.primary {
            return Err(PRIMARY_KEY_NOT_DROPPED.into());
        }
        repository
            .query(&repository.drop_index_sql(table, &index.name))
            .await?;
        Ok(())
    }

    /// Adds a foreign key to the table. Not on `SQLite`.
    pub async fn add_foreign_key(
        &self,
        id: &str,
        table: &TableRef,
        key: &NewForeignKey,
    ) -> AppResult<()> {
        let name = Self::key_name("foreign key", &key.name)?;
        let repository = self.repository(id)?;
        repository.check_foreign_key_change()?;
        if key.columns.len() != key.ref_columns.len() {
            return Err("Pick as many referenced columns as columns".into());
        }
        let columns = repository.columns(table).await?;
        let ref_table = Self::known_table(repository.as_ref(), &key.ref_table).await?;
        let ref_columns = repository.columns(&ref_table).await?;
        let sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) \
            ON DELETE {} ON UPDATE {}",
            repository.table_name(table),
            repository.quote(name),
            Self::column_list(repository.as_ref(), &columns, &key.columns)?,
            repository.table_name(&ref_table),
            Self::column_list(repository.as_ref(), &ref_columns, &key.ref_columns)?,
            key.on_delete.sql(),
            key.on_update.sql()
        );
        repository.query(&sql).await?;
        Ok(())
    }

    /// Drops a foreign key of the table. Not on `SQLite`.
    pub async fn drop_foreign_key(&self, id: &str, table: &TableRef, name: &str) -> AppResult<()> {
        let repository = self.repository(id)?;
        repository.check_foreign_key_change()?;
        let keys = repository.foreign_keys(table).await?;
        if !keys.iter().any(|key| key.name == name) {
            return Err(AppError::Message(format!("Unknown foreign key: {name}")));
        }
        let sql =
            repository.drop_foreign_key_sql(&repository.table_name(table), &repository.quote(name));
        repository.query(&sql).await?;
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

    /// The triggers and routines of the current database.
    pub async fn objects(&self, id: &str) -> AppResult<Vec<DbObject>> {
        self.repository(id)?.objects().await
    }

    /// The `CREATE` statement of a view, trigger or routine.
    pub async fn object_definition(&self, id: &str, object: &DbObject) -> AppResult<String> {
        let repository = self.repository(id)?;
        if object.kind == VIEW {
            let view = TableRef {
                schema: object.schema.clone(),
                name: object.name.clone(),
                kind: VIEW.to_owned(),
            };
            let view = Self::known_table(repository.as_ref(), &view).await?;
            return repository
                .create_sql(&view)
                .await?
                .ok_or_else(|| AppError::Message(format!("No definition for {}", view.name)));
        }
        let object = Self::known_object(repository.as_ref(), object).await?;
        repository.object_sql(&object).await
    }

    /// Removes a trigger or routine. A view is dropped with `drop_table`.
    pub async fn drop_object(&self, id: &str, object: &DbObject) -> AppResult<()> {
        let repository = self.repository(id)?;
        let object = Self::known_object(repository.as_ref(), object).await?;
        repository
            .query(&repository.drop_object_sql(&object))
            .await?;
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

    /// The login accounts of the server. `SQLite` has none and gives an empty list.
    pub async fn list_users(&self, id: &str) -> AppResult<UserList> {
        self.repository(id)?.users().await
    }

    /// Creates a login account, then gives it its access level.
    pub async fn create_user(&self, id: &str, user: &NewUser) -> AppResult<()> {
        if user.name.is_empty() {
            return Err("The user needs a name".into());
        }
        Self::check_text("name", &user.name)?;
        Self::check_text("host", user.host.as_deref().unwrap_or_default())?;
        Self::check_password(&user.password)?;
        Self::check_level(user.access)?;
        let (repository, list) = self.user_list(id).await?;
        if !list.can_manage {
            return Err(CANNOT_MANAGE_USERS.into());
        }
        let grant = user.access != AccessLevel::None;
        if grant {
            // Check before the account exists, so nothing is left half done.
            repository.check_grantable().await?;
        }
        let host = user.host.as_deref();
        repository
            .create_user(&user.name, host, &user.password)
            .await?;
        if !grant {
            return Ok(());
        }
        match repository
            .set_user_access(&user.name, host, user.access)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => Err(AppError::Message(format!(
                "The user was created, but its access could not be set: {error}"
            ))),
        }
    }

    /// Removes a login account. Never the one this connection is logged in as.
    pub async fn drop_user(&self, id: &str, name: &str, host: Option<&str>) -> AppResult<()> {
        let (repository, list) = self.user_list(id).await?;
        if !list.can_manage {
            return Err(CANNOT_MANAGE_USERS.into());
        }
        let user = Self::user(&list.users, name, host)?;
        if user.current {
            return Err("You cannot remove your own account".into());
        }
        repository.drop_user(&user.name, user.host.as_deref()).await
    }

    /// Sets what an account may do in the current database: nothing, read, or read and write.
    pub async fn set_user_access(
        &self,
        id: &str,
        name: &str,
        host: Option<&str>,
        access: AccessLevel,
    ) -> AppResult<()> {
        Self::check_level(access)?;
        let (repository, list) = self.user_list(id).await?;
        if !list.can_manage {
            return Err(CANNOT_MANAGE_USERS.into());
        }
        let user = Self::user(&list.users, name, host)?;
        if user.current {
            return Err("You cannot change the access of your own account".into());
        }
        repository
            .set_user_access(&user.name, user.host.as_deref(), access)
            .await
    }

    /// Gives an account a new password. Everyone may change their own.
    pub async fn set_user_password(
        &self,
        id: &str,
        name: &str,
        host: Option<&str>,
        password: &str,
    ) -> AppResult<()> {
        Self::check_password(password)?;
        let (repository, list) = self.user_list(id).await?;
        let user = match Self::user(&list.users, name, host) {
            Ok(user) => user,
            // An account that may not manage users does not see the other accounts.
            Err(_) if !list.can_manage => return Err(CANNOT_MANAGE_USERS.into()),
            Err(error) => return Err(error),
        };
        if !user.current && !list.can_manage {
            return Err(CANNOT_MANAGE_USERS.into());
        }
        repository
            .set_user_password(&user.name, user.host.as_deref(), password)
            .await
    }

    /// The repository of a tab and its accounts, as the database lists them now.
    async fn user_list(&self, id: &str) -> AppResult<(Arc<dyn Repository>, UserList)> {
        let connection = self.connection(id)?;
        if connection.config.driver == Driver::Sqlite {
            return Err(NO_USER_ACCOUNTS.into());
        }
        let list = connection.repository.users().await?;
        Ok((connection.repository, list))
    }

    /// Finds an account in the list of the database. Stops names that are not in it.
    /// A missing host means "any host", as it does when an account is created.
    fn user<'list>(
        users: &'list [DbUser],
        name: &str,
        host: Option<&str>,
    ) -> AppResult<&'list DbUser> {
        let host = host.filter(|host| !host.is_empty()).unwrap_or(ANY_HOST);
        for user in users {
            // PostgreSQL accounts have no host.
            let same_host = user.host.as_deref().is_none_or(|listed| listed == host);
            if user.name == name && same_host {
                return Ok(user);
            }
        }
        Err(AppError::Message(format!("Unknown user: {name}")))
    }

    /// Stops characters that have no place in a name, host or password.
    fn check_text(label: &str, value: &str) -> AppResult<()> {
        if value.chars().any(char::is_control) {
            return Err(AppError::Message(format!(
                "The {label} must not hold line breaks or other control characters"
            )));
        }
        Ok(())
    }

    fn check_password(password: &str) -> AppResult<()> {
        if password.is_empty() {
            return Err("The password must not be empty".into());
        }
        Self::check_text("password", password)
    }

    /// Only these three levels can be set. `Admin` and `Custom` are only reported.
    fn check_level(access: AccessLevel) -> AppResult<()> {
        if matches!(access, AccessLevel::Admin | AccessLevel::Custom) {
            return Err("The access must be none, read or write".into());
        }
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

    /// The trigger or routine as the catalog has it. Its kind and argument types go
    /// into the SQL, so they must come from the database, never from the caller.
    async fn known_object(repository: &dyn Repository, object: &DbObject) -> AppResult<DbObject> {
        for known in repository.objects().await? {
            if known == *object {
                return Ok(known);
            }
        }
        Err(AppError::Message(format!(
            "Unknown object: {}",
            object.name
        )))
    }

    fn check_table_name(name: &str) -> AppResult<()> {
        if name.trim().is_empty() {
            return Err("The table needs a name".into());
        }
        Ok(())
    }

    /// The trimmed name of a new index or foreign key. Fails when it is empty.
    fn key_name<'name>(what: &str, name: &'name str) -> AppResult<&'name str> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::Message(format!("The {what} needs a name")));
        }
        Ok(name)
    }

    /// The quoted names of `names`, joined with commas. Each must be a column of the table.
    fn column_list(
        repository: &dyn Repository,
        columns: &[ColumnInfo],
        names: &[String],
    ) -> AppResult<String> {
        if names.is_empty() {
            return Err("Pick at least one column".into());
        }
        let mut quoted = Vec::with_capacity(names.len());
        for name in names {
            quoted.push(repository.quote(&Self::column(columns, name)?.name));
        }
        Ok(quoted.join(", "))
    }

    /// One column as `ADD COLUMN` and `CREATE TABLE` write it: name, type, null rule, default.
    /// The type and the default are raw SQL from the user. Only the name is quoted.
    fn column_definition(
        repository: &dyn Repository,
        column: &ColumnSpec,
        nullable: bool,
    ) -> AppResult<String> {
        Self::check_spec(column)?;
        let mut definition = format!("{} {}", repository.quote(&column.name), column.data_type);
        if !nullable {
            definition.push_str(" NOT NULL");
        }
        if let Some(default) = &column.default {
            definition.push_str(" DEFAULT ");
            definition.push_str(default);
        }
        Ok(definition)
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

    /// Turns built SQL into one statement of a batch. An error gets the label in front.
    fn statement(label: String, built: AppResult<(String, Params)>) -> AppResult<Statement> {
        match built {
            Ok((sql, params)) => Ok(Statement { label, sql, params }),
            Err(error) => Err(not_saved(&label, &error)),
        }
    }

    /// Builds the UPDATE for the one row that `key` points at, with its bound values.
    fn update_sql(
        repository: &dyn Repository,
        table: &TableRef,
        columns: &[ColumnInfo],
        key: Values,
        changes: Values,
    ) -> AppResult<(String, Params)> {
        let mut params = Vec::new();
        let mut assignments = Vec::with_capacity(changes.len());
        for (name, value) in changes {
            let column = Self::column(columns, &name)?;
            params.push(value);
            assignments.push(Self::equals(repository, column, params.len()));
        }
        let condition = Self::key_condition(repository, columns, key, &mut params)?;
        let sql = format!(
            "UPDATE {} SET {} WHERE {condition}",
            repository.table_name(table),
            assignments.join(", ")
        );
        Ok((sql, params))
    }

    /// Counts the statements that changed a row.
    /// Zero is fine: the row is gone, or `MySQL` saw the same value as before.
    fn changed(counts: &[u64]) -> usize {
        counts.iter().filter(|count| **count == 1).count()
    }

    /// Builds the DELETE for the one row that `key` points at, with its bound values.
    fn delete_sql(
        repository: &dyn Repository,
        table: &TableRef,
        columns: &[ColumnInfo],
        key: Values,
    ) -> AppResult<(String, Params)> {
        let mut params = Vec::new();
        let condition = Self::key_condition(repository, columns, key, &mut params)?;
        let sql = format!(
            "DELETE FROM {} WHERE {condition}",
            repository.table_name(table)
        );
        Ok((sql, params))
    }

    /// Builds the INSERT for one new row, with its bound values.
    fn insert_sql(
        repository: &dyn Repository,
        table: &TableRef,
        columns: &[ColumnInfo],
        values: Values,
    ) -> AppResult<(String, Params)> {
        let mut params = Vec::new();
        let mut names = Vec::with_capacity(values.len());
        let mut placeholders = Vec::with_capacity(values.len());
        for (name, value) in values {
            let column = Self::column(columns, &name)?;
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
        Ok((sql, params))
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
