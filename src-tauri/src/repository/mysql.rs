//! `MySQL` and `MariaDB` access.

use async_trait::async_trait;
use sqlx::mysql::{MySqlConnectOptions, MySqlConnection, MySqlPool, MySqlQueryResult, MySqlRow};
use sqlx::{AssertSqlSafe, MySql, Row as _};

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::{
    Repository, ServerId, Statement, bytes_to_text, cell, collect, default_sql, definition,
    execute, execute_all, first_column, foreign_keys, indexes, keyword, kind, objects,
    password_safe, pool_options, relations, run_on_new_connection, run_script, single,
};
use crate::constants::{
    ANY_HOST, MYSQL_DATABASE_PRIVILEGES, MYSQL_HOST_MAX, MYSQL_SYSTEM_USERS, MYSQL_USER_NAME_MAX,
};
use crate::error::{AppError, AppResult};
use crate::model::{
    AccessLevel, ColumnInfo, ColumnSpec, ConnectionConfig, DbObject, DbUser, ForeignKey, IndexInfo,
    Relation, ResultSet, TableRef, UserList,
};

/// The right that is no right. `MySQL` lists it for an account that has nothing.
const USAGE: &str = "USAGE";

/// One row per foreign key column, in the shape `relations` reads.
// ponytail: a key that points into another database is left out. The diagram
// shows one database. Fill `ref_schema` here if such keys must show up.
const FOREIGN_KEYS_SQL: &str = "SELECT NULL, TABLE_NAME, CONSTRAINT_NAME, CONSTRAINT_NAME, \
        COLUMN_NAME, NULL, REFERENCED_TABLE_NAME, REFERENCED_COLUMN_NAME \
    FROM information_schema.KEY_COLUMN_USAGE \
    WHERE TABLE_SCHEMA = DATABASE() AND REFERENCED_TABLE_NAME IS NOT NULL \
        AND REFERENCED_TABLE_SCHEMA = DATABASE() \
    ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION";

/// One row per column of every unique index. The primary key is one of them.
const UNIQUE_SQL: &str = "SELECT NULL, TABLE_NAME, INDEX_NAME, COLUMN_NAME \
    FROM information_schema.STATISTICS \
    WHERE TABLE_SCHEMA = DATABASE() AND NON_UNIQUE = 0";

/// The triggers, then the procedures and functions, in the shape `objects` reads.
/// Two statements and not one UNION: the columns of the two tables can have
/// different collations, and `MySQL` refuses a UNION of those.
const OBJECTS_SQL: &str = "SELECT 'trigger', NULL, TRIGGER_NAME, EVENT_OBJECT_TABLE, NULL \
    FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() ORDER BY TRIGGER_NAME; \
    SELECT LOWER(ROUTINE_TYPE), NULL, ROUTINE_NAME, NULL, NULL \
    FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA = DATABASE() \
    ORDER BY ROUTINE_TYPE, ROUTINE_NAME";

/// One grant inside the current database: on the whole database, or on one table.
struct Grant {
    /// The account, as `information_schema` writes it: `'name'@'host'`.
    grantee: String,
    /// What the grant is on, ready for SQL: `` `db`.* `` or `` `db`.`table` ``.
    target: String,
    on_table: bool,
    privileges: Vec<String>,
    /// The account may pass these rights on.
    grantable: bool,
}

pub struct MysqlRepository {
    pool: MySqlPool,
    has_database: bool,
    /// The settings of the pool. Cancel opens its own connection with them.
    options: MySqlConnectOptions,
    server_id: ServerId,
}

impl MysqlRepository {
    pub async fn connect(config: &ConnectionConfig) -> AppResult<Self> {
        let has_database = !config.database.is_empty();
        // sqlx changes sql_mode and the time zone by default. A client must show
        // the server as it is, so we turn both off.
        let mut options = MySqlConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .username(&config.user)
            .password(&config.password)
            .pipes_as_concat(false)
            .no_engine_substitution(false)
            .timezone(None::<String>);
        if has_database {
            options = options.database(&config.database);
        }
        let server_id = ServerId::default();
        let seen = Arc::clone(&server_id);
        let pool = pool_options()
            .after_connect(move |connection, _| {
                let seen = Arc::clone(&seen);
                Box::pin(async move {
                    let id: i64 = sqlx::query_scalar("SELECT CAST(CONNECTION_ID() AS SIGNED)")
                        .fetch_one(connection)
                        .await?;
                    seen.store(id, Ordering::Relaxed);
                    Ok(())
                })
            })
            .connect_with(options.clone())
            .await?;
        Ok(Self {
            pool,
            has_database,
            options,
            server_id,
        })
    }

    /// The definition of a column as `SHOW CREATE TABLE` prints it, without the name.
    /// Example: ``varchar(20) NOT NULL DEFAULT 'x' COMMENT 'note'``.
    async fn column_definition(&self, table: &TableRef, column: &str) -> AppResult<String> {
        let create = self.create_sql(table).await?.unwrap_or_default();
        let prefix = format!("{} ", self.quote(column));
        for line in create.lines() {
            if let Some(definition) = line.trim().strip_prefix(&prefix) {
                return Ok(definition.trim_end_matches(',').to_owned());
            }
        }
        Err(AppError::Message(format!(
            "Cannot read the definition of column {column}"
        )))
    }

    /// Changes the type and the null rule inside a column definition.
    /// Everything else (default, comment, auto increment) stays as it is.
    // ponytail: plain text surgery. It can go wrong when a comment or default
    // holds the words NOT NULL. Use a real parser if that ever bites.
    fn redefine(definition: &str, old: &ColumnInfo, new: &ColumnSpec) -> AppResult<String> {
        let mut definition = definition.to_owned();
        if new.data_type != old.data_type {
            // The definition starts with the type, written like SHOW COLUMNS writes it.
            let length = old.data_type.len();
            let starts_with_type = definition
                .get(..length)
                .is_some_and(|head| head.eq_ignore_ascii_case(&old.data_type));
            if !starts_with_type {
                return Err("Cannot read the current type of the column".into());
            }
            definition.replace_range(..length, &new.data_type);
            if !Self::is_text_type(&new.data_type) {
                // A charset on a number or date column is an error.
                definition = Self::without_attribute(&definition, " CHARACTER SET ");
                definition = Self::without_attribute(&definition, " COLLATE ");
            }
        }
        if new.nullable != old.nullable {
            definition = definition.replacen(" NOT NULL", "", 1);
            if new.nullable {
                definition.push_str(" NULL");
            } else {
                definition = definition.replacen(" DEFAULT NULL", "", 1);
                definition.push_str(" NOT NULL");
            }
        }
        Ok(definition)
    }

    fn is_text_type(data_type: &str) -> bool {
        let data_type = data_type.to_ascii_lowercase();
        ["char", "text", "enum", "set"]
            .iter()
            .any(|kind| data_type.contains(kind))
    }

    /// Removes a keyword and the one word after it, like ` COLLATE utf8mb4_bin`.
    fn without_attribute(definition: &str, keyword: &str) -> String {
        let Some(start) = definition.find(keyword) else {
            return definition.to_owned();
        };
        let value_start = start + keyword.len();
        let value_length = definition[value_start..]
            .find(' ')
            .unwrap_or(definition.len() - value_start);
        format!(
            "{}{}",
            &definition[..start],
            &definition[value_start + value_length..]
        )
    }

    /// The account this connection is logged in as: name and host.
    async fn current_account(&self) -> AppResult<(String, String)> {
        let set = single(self.query("SELECT CURRENT_USER()").await?);
        let account = set.rows.first().map(|row| cell(row, 0)).unwrap_or_default();
        // A host never holds an `@`, a name can.
        let (name, host) = account.rsplit_once('@').unwrap_or((&account, ""));
        Ok((name.to_owned(), host.to_owned()))
    }

    /// The selected database. `None` when the connection has none.
    async fn database(&self) -> AppResult<Option<String>> {
        let set = single(self.query("SELECT DATABASE()").await?);
        Ok(set
            .rows
            .first()
            .and_then(|row| row.first().cloned().flatten()))
    }

    /// Writes a text value as a SQL string, in the way the session reads strings.
    ///
    /// With the `NO_BACKSLASH_ESCAPES` mode a backslash is plain text and must
    /// not be doubled. A doubled one would silently change a password.
    async fn text(&self, value: &str) -> AppResult<String> {
        let sql = "SELECT @@sql_mode LIKE '%NO_BACKSLASH_ESCAPES%'";
        let set = single(self.query(sql).await?);
        let plain_backslash = set.rows.first().is_some_and(|row| cell(row, 0) == "1");
        let value = if plain_backslash {
            value.to_owned()
        } else {
            value.replace('\\', "\\\\")
        };
        Ok(format!("'{}'", value.replace('\'', "''")))
    }

    /// The host of an account. An empty host means every host. Hosts are stored in lower case.
    fn host(host: Option<&str>) -> String {
        match host {
            Some(host) if !host.is_empty() => host.to_lowercase(),
            _ => ANY_HOST.to_owned(),
        }
    }

    /// An account, ready for SQL: `'name'@'host'`.
    async fn account(&self, name: &str, host: Option<&str>) -> AppResult<String> {
        let host = Self::host(host);
        Ok(format!(
            "{}@{}",
            self.text(name).await?,
            self.text(&host).await?
        ))
    }

    /// An account as `information_schema` writes it. That is not valid SQL: it escapes nothing.
    fn grantee(name: &str, host: &str) -> String {
        format!("'{name}'@'{host}'")
    }

    /// Splits a list that `GROUP_CONCAT` made.
    fn split(list: &str) -> Vec<String> {
        list.split(',').map(str::to_owned).collect()
    }

    /// The database rights each account holds on every database, by grantee.
    /// An account without such rights is not in the map.
    async fn global_rights(&self) -> AppResult<HashMap<String, Vec<String>>> {
        let sql = format!(
            "SELECT GRANTEE, GROUP_CONCAT(PRIVILEGE_TYPE) \
            FROM information_schema.user_privileges \
            WHERE PRIVILEGE_TYPE IN ('{}') GROUP BY GRANTEE",
            MYSQL_DATABASE_PRIVILEGES.join("', '")
        );
        let set = single(self.query(&sql).await?);
        let mut rights = HashMap::with_capacity(set.rows.len());
        for row in &set.rows {
            rights.insert(cell(row, 0), Self::split(&cell(row, 1)));
        }
        Ok(rights)
    }

    /// A database name as a pattern that matches only this name.
    /// In a grant on a database, `_` and `%` are wildcards.
    fn pattern(database: &str) -> String {
        database
            .replace('\\', "\\\\")
            .replace('_', "\\_")
            .replace('%', "\\%")
    }

    /// All grants inside `database`, of every account this connection may see.
    // ponytail: a grant on a wildcard pattern that also matches this database, like
    // `shop%`, is not found. Grants on routines are not found either. Match patterns
    // and read `mysql.procs_priv` if such grants must show up.
    async fn grants(&self, database: &str) -> AppResult<Vec<Grant>> {
        let name = self.text(database).await?;
        let pattern = self.text(&Self::pattern(database)).await?;
        let sql = format!(
            "SELECT GRANTEE, TABLE_SCHEMA, NULL, GROUP_CONCAT(PRIVILEGE_TYPE), MAX(IS_GRANTABLE) \
            FROM information_schema.schema_privileges \
            WHERE TABLE_SCHEMA IN ({name}, {pattern}) GROUP BY GRANTEE, TABLE_SCHEMA \
            UNION ALL \
            SELECT GRANTEE, TABLE_SCHEMA, TABLE_NAME, GROUP_CONCAT(DISTINCT PRIVILEGE_TYPE), \
                MAX(IS_GRANTABLE) \
            FROM (SELECT GRANTEE, TABLE_SCHEMA, TABLE_NAME, PRIVILEGE_TYPE, IS_GRANTABLE \
                    FROM information_schema.table_privileges \
                UNION ALL \
                SELECT GRANTEE, TABLE_SCHEMA, TABLE_NAME, PRIVILEGE_TYPE, IS_GRANTABLE \
                    FROM information_schema.column_privileges) table_grants \
            WHERE TABLE_SCHEMA = {name} GROUP BY GRANTEE, TABLE_SCHEMA, TABLE_NAME"
        );
        let set = single(self.query(&sql).await?);
        let mut grants = Vec::with_capacity(set.rows.len());
        for row in &set.rows {
            let schema = self.quote(&cell(row, 1));
            let table = row.get(2).cloned().flatten();
            let target = match &table {
                Some(table) => format!("{schema}.{}", self.quote(table)),
                None => format!("{schema}.*"),
            };
            grants.push(Grant {
                grantee: cell(row, 0),
                target,
                on_table: table.is_some(),
                privileges: Self::split(&cell(row, 3)),
                grantable: cell(row, 4) == "YES",
            });
        }
        Ok(grants)
    }

    /// The access level of one account, from its global rights and its grants in the database.
    fn level(global: &[String], grants: &[&Grant]) -> AccessLevel {
        let mut held: BTreeSet<&str> = global.iter().map(String::as_str).collect();
        let mut table_grants = false;
        for grant in grants {
            if grant.on_table {
                table_grants = true;
            } else {
                held.extend(grant.privileges.iter().map(String::as_str));
            }
        }
        held.remove(USAGE);
        let has_all = MYSQL_DATABASE_PRIVILEGES
            .iter()
            .all(|privilege| held.contains(privilege));
        if has_all {
            return AccessLevel::Admin;
        }
        if table_grants {
            return AccessLevel::Custom;
        }
        if held.is_empty() {
            return AccessLevel::None;
        }
        // Many read-only accounts also have SHOW VIEW. It changes no data, so we let it pass.
        held.remove("SHOW VIEW");
        // A `BTreeSet` is sorted, so the order here is fixed.
        match held.into_iter().collect::<Vec<_>>()[..] {
            ["SELECT"] => AccessLevel::Read,
            ["DELETE", "INSERT", "SELECT", "UPDATE"] => AccessLevel::Write,
            _ => AccessLevel::Custom,
        }
    }

    /// The name and host of every account, without the built-in ones.
    // ponytail: a MySQL 8 role looks like a normal account, so it is listed.
    // A MariaDB role has an empty host, so those are skipped.
    async fn accounts(&self) -> AppResult<Vec<(String, String)>> {
        let sql = "SELECT User, Host FROM mysql.user ORDER BY User, Host";
        let set = single(self.query(sql).await?);
        let mut accounts = Vec::with_capacity(set.rows.len());
        for row in &set.rows {
            let (name, host) = (cell(row, 0), cell(row, 1));
            if !host.is_empty() && !MYSQL_SYSTEM_USERS.contains(&name.as_str()) {
                accounts.push((name, host));
            }
        }
        Ok(accounts)
    }

    /// Can this account create and drop users? It needs the global `CREATE USER` right.
    // ponytail: a right that comes from a role is not seen. Read the grants of the
    // active roles if admins log in through roles.
    async fn can_create_users(&self, grantee: &str) -> AppResult<bool> {
        let sql = "SELECT GRANTEE FROM information_schema.user_privileges \
            WHERE PRIVILEGE_TYPE = 'CREATE USER'";
        let set = single(self.query(sql).await?);
        Ok(first_column(&set).iter().any(|found| found == grantee))
    }

    /// Runs a statement that holds a password. The error never shows the password.
    async fn run_secret(&self, sql: &str, password: &str) -> AppResult<()> {
        match self.query(sql).await {
            Ok(_) => Ok(()),
            Err(error) => Err(password_safe(error, password)),
        }
    }

    /// `raw_sql` uses the text protocol, so every value arrives as text bytes.
    fn read(row: &MySqlRow, index: usize) -> Result<Option<String>, sqlx::Error> {
        let bytes: Option<&[u8]> = row.try_get_unchecked(index)?;
        Ok(bytes.map(bytes_to_text))
    }
}

#[async_trait]
impl Repository for MysqlRepository {
    fn quote(&self, identifier: &str) -> String {
        format!("`{}`", identifier.replace('`', "``"))
    }

    fn placeholder(&self, _index: usize, _data_type: &str) -> String {
        "?".to_owned()
    }

    /// `MySQL` reads a backslash inside a text value as an escape, so we double it.
    /// A quote is doubled, never written as `\'`. That is safe in every `sql_mode`.
    // ponytail: with the `NO_BACKSLASH_ESCAPES` sql_mode a backslash is plain text, so
    // a value with a backslash then matches the wrong rows (two backslashes). It still
    // cannot break out of the quotes. Read `@@sql_mode` on connect if this matters.
    fn literal(&self, value: &str) -> String {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
    }

    fn default_values(&self) -> &'static str {
        "() VALUES ()"
    }

    fn rename_table_sql(&self, table: &TableRef, new_name: &str) -> String {
        format!(
            "RENAME TABLE {} TO {}",
            self.table_name(table),
            self.quote(new_name)
        )
    }

    fn auto_increment(&self) -> &'static str {
        "AUTO_INCREMENT"
    }

    fn drop_index_sql(&self, table: &TableRef, name: &str) -> String {
        format!(
            "DROP INDEX {} ON {}",
            self.quote(name),
            self.table_name(table)
        )
    }

    /// `MariaDB` has no `DROP CONSTRAINT` for foreign keys.
    fn drop_foreign_key_sql(&self, table: &str, name: &str) -> String {
        format!("ALTER TABLE {table} DROP FOREIGN KEY {name}")
    }

    async fn query(&self, sql: &str) -> AppResult<Vec<ResultSet>> {
        let stream = sqlx::raw_sql(AssertSqlSafe(sql.to_owned())).fetch_many(&self.pool);
        collect::<MySql>(stream, Self::read, MySqlQueryResult::rows_affected).await
    }

    async fn execute(&self, sql: &str, params: Vec<Option<String>>) -> AppResult<u64> {
        execute(&self.pool, sql, params, MySqlQueryResult::rows_affected).await
    }

    // ponytail: a rollback does nothing on MyISAM tables, they have no transactions.
    // Changes made before the failed one stay there. Only InnoDB is all or nothing.
    async fn execute_all(&self, statements: Vec<Statement>) -> AppResult<Vec<u64>> {
        execute_all(&self.pool, statements, MySqlQueryResult::rows_affected).await
    }

    async fn run_script(&self, statements: Vec<Statement>) -> AppResult<()> {
        run_script(&self.pool, statements).await
    }

    async fn databases(&self) -> AppResult<Vec<String>> {
        Ok(first_column(&single(self.query("SHOW DATABASES").await?)))
    }

    async fn tables(&self) -> AppResult<Vec<TableRef>> {
        if !self.has_database {
            return Ok(Vec::new());
        }
        let set = single(self.query("SHOW FULL TABLES").await?);
        let mut tables = Vec::with_capacity(set.rows.len());
        for row in &set.rows {
            tables.push(TableRef {
                schema: None,
                name: cell(row, 0),
                kind: kind(&cell(row, 1)),
            });
        }
        Ok(tables)
    }

    async fn columns(&self, table: &TableRef) -> AppResult<Vec<ColumnInfo>> {
        let sql = format!("SHOW COLUMNS FROM {}", self.table_name(table));
        let set = single(self.query(&sql).await?);
        let mut columns = Vec::with_capacity(set.rows.len());
        // SHOW COLUMNS gives: Field, Type, Null, Key, Default, Extra.
        for row in &set.rows {
            columns.push(ColumnInfo {
                name: cell(row, 0),
                data_type: cell(row, 1),
                nullable: cell(row, 2) == "YES",
                primary_key: cell(row, 3) == "PRI",
                default: row.get(4).cloned().flatten(),
                extra: cell(row, 5),
            });
        }
        Ok(columns)
    }

    async fn relations(&self) -> AppResult<Vec<Relation>> {
        let keys = single(self.query(FOREIGN_KEYS_SQL).await?);
        let unique = single(self.query(UNIQUE_SQL).await?);
        Ok(relations(&keys, &unique))
    }

    /// A part of an index on an expression has no column name. It shows as empty text.
    async fn indexes(&self, table: &TableRef) -> AppResult<Vec<IndexInfo>> {
        let sql = format!(
            "SELECT INDEX_NAME, COLUMN_NAME, CASE WHEN NON_UNIQUE = 0 THEN 'YES' ELSE 'NO' END, \
                CASE WHEN INDEX_NAME = 'PRIMARY' THEN 'YES' ELSE 'NO' END \
            FROM information_schema.STATISTICS \
            WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = {} \
            ORDER BY INDEX_NAME <> 'PRIMARY', INDEX_NAME, SEQ_IN_INDEX",
            self.literal(&table.name)
        );
        Ok(indexes(&single(self.query(&sql).await?)))
    }

    async fn foreign_keys(&self, table: &TableRef) -> AppResult<Vec<ForeignKey>> {
        let sql = format!(
            "SELECT k.CONSTRAINT_NAME, k.CONSTRAINT_NAME, k.COLUMN_NAME, \
                CASE WHEN k.REFERENCED_TABLE_SCHEMA <> DATABASE() THEN k.REFERENCED_TABLE_SCHEMA END, \
                k.REFERENCED_TABLE_NAME, k.REFERENCED_COLUMN_NAME, r.DELETE_RULE, r.UPDATE_RULE \
            FROM information_schema.KEY_COLUMN_USAGE k \
            JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
                ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA \
                AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME AND r.TABLE_NAME = k.TABLE_NAME \
            WHERE k.TABLE_SCHEMA = DATABASE() AND k.TABLE_NAME = {} \
                AND k.REFERENCED_TABLE_NAME IS NOT NULL \
            ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION",
            self.literal(&table.name)
        );
        Ok(foreign_keys(&single(self.query(&sql).await?)))
    }

    /// Also works for a view. Then it gives the `CREATE VIEW` statement.
    async fn create_sql(&self, table: &TableRef) -> AppResult<Option<String>> {
        let sql = format!("SHOW CREATE TABLE {}", self.table_name(table));
        let set = single(self.query(&sql).await?);
        Ok(set.rows.first().map(|row| cell(row, 1)))
    }

    async fn objects(&self) -> AppResult<Vec<DbObject>> {
        let mut list = Vec::new();
        for set in self.query(OBJECTS_SQL).await? {
            list.extend(objects(&set));
        }
        Ok(list)
    }

    /// `SHOW CREATE TRIGGER`, `PROCEDURE` and `FUNCTION` all give the statement in the third column.
    async fn object_sql(&self, object: &DbObject) -> AppResult<String> {
        let sql = format!(
            "SHOW CREATE {} {}",
            keyword(object),
            self.object_name(object)
        );
        definition(&single(self.query(&sql).await?), 2, &object.name)
    }

    async fn modify_column_sql(
        &self,
        table: &TableRef,
        old: &ColumnInfo,
        new: &ColumnSpec,
    ) -> AppResult<Vec<String>> {
        let table_sql = self.table_name(table);
        let column = self.quote(&old.name);
        let mut statements = Vec::new();
        if new.data_type != old.data_type || new.nullable != old.nullable {
            // MODIFY needs the full definition, so we start from the current one.
            let current = self.column_definition(table, &old.name).await?;
            let definition = Self::redefine(&current, old, new)?;
            statements.push(format!(
                "ALTER TABLE {table_sql} MODIFY COLUMN {column} {definition}"
            ));
        }
        statements.extend(default_sql(&table_sql, &column, old, new));
        Ok(statements)
    }

    async fn users(&self) -> AppResult<UserList> {
        let me = self.current_account().await?;
        let mut can_manage = self.can_create_users(&Self::grantee(&me.0, &me.1)).await?;
        let mut accounts = vec![me.clone()];
        if can_manage {
            match self.accounts().await {
                Ok(all) => accounts = all,
                // The database says no: this account may not read the user table.
                // Then it sees only itself.
                Err(AppError::Database(sqlx::Error::Database(_))) => can_manage = false,
                Err(error) => return Err(error),
            }
        }
        let global = self.global_rights().await?;
        let grants = match self.database().await? {
            Some(database) => self.grants(&database).await?,
            None => Vec::new(),
        };
        let mut users = Vec::with_capacity(accounts.len());
        for (name, host) in accounts {
            let grantee = Self::grantee(&name, &host);
            let own_global = global.get(&grantee).map_or(&[][..], Vec::as_slice);
            let own_grants: Vec<&Grant> = grants
                .iter()
                .filter(|grant| grant.grantee == grantee)
                .collect();
            users.push(DbUser {
                access: Self::level(own_global, &own_grants),
                current: name == me.0 && host == me.1,
                name,
                host: Some(host),
            });
        }
        Ok(UserList { can_manage, users })
    }

    async fn check_grantable(&self) -> AppResult<()> {
        match self.database().await? {
            Some(_) => Ok(()),
            None => Err("Choose a database first. Access is set per database.".into()),
        }
    }

    async fn create_user(&self, name: &str, host: Option<&str>, password: &str) -> AppResult<()> {
        if name.chars().count() > MYSQL_USER_NAME_MAX {
            return Err(AppError::Message(format!(
                "The user name is too long. The limit is {MYSQL_USER_NAME_MAX} characters."
            )));
        }
        if Self::host(host).chars().count() > MYSQL_HOST_MAX {
            return Err(AppError::Message(format!(
                "The host is too long. The limit is {MYSQL_HOST_MAX} characters."
            )));
        }
        let sql = format!(
            "CREATE USER {} IDENTIFIED BY {}",
            self.account(name, host).await?,
            self.text(password).await?
        );
        self.run_secret(&sql, password).await
    }

    async fn drop_user(&self, name: &str, host: Option<&str>) -> AppResult<()> {
        let sql = format!("DROP USER {}", self.account(name, host).await?);
        self.query(&sql).await?;
        Ok(())
    }

    async fn set_user_password(
        &self,
        name: &str,
        host: Option<&str>,
        password: &str,
    ) -> AppResult<()> {
        let secret = self.text(password).await?;
        let (my_name, my_host) = self.current_account().await?;
        let own = name == my_name && Self::host(host) == my_host;
        let version = single(self.query("SELECT VERSION()").await?);
        let sql = if own && first_column(&version).concat().contains("MariaDB") {
            // MariaDB wants the CREATE USER right for ALTER USER, even on your own account.
            // This form needs no right. MySQL 8 does not know it.
            format!("SET PASSWORD = PASSWORD({secret})")
        } else {
            format!(
                "ALTER USER {} IDENTIFIED BY {secret}",
                self.account(name, host).await?
            )
        };
        self.run_secret(&sql, password).await
    }

    // ponytail: the statements run one by one, MySQL cannot roll back a GRANT.
    // If one fails, the earlier ones stay. Setting the access again repairs it.
    async fn set_user_access(
        &self,
        name: &str,
        host: Option<&str>,
        access: AccessLevel,
    ) -> AppResult<()> {
        self.check_grantable().await?;
        let database = self.database().await?.unwrap_or_default();
        let grantee = Self::grantee(name, &Self::host(host));
        if self.global_rights().await?.contains_key(&grantee) {
            return Err(
                "This account has global rights. They count in every database, \
                so its access to one database cannot be changed here."
                    .into(),
            );
        }
        let account = self.account(name, host).await?;
        for grant in self.grants(&database).await? {
            if grant.grantee != grantee {
                continue;
            }
            let target = &grant.target;
            // REVOKE ALL fails when the grant holds no right at all.
            if grant.privileges.iter().any(|privilege| privilege != USAGE) {
                let sql = format!("REVOKE ALL PRIVILEGES ON {target} FROM {account}");
                self.query(&sql).await?;
            }
            // ALL does not cover the right to pass rights on.
            if grant.grantable {
                let sql = format!("REVOKE GRANT OPTION ON {target} FROM {account}");
                self.query(&sql).await?;
            }
        }
        let privileges = match access {
            AccessLevel::Read => "SELECT",
            AccessLevel::Write => "SELECT, INSERT, UPDATE, DELETE",
            _ => return Ok(()),
        };
        let pattern = self.quote(&Self::pattern(&database));
        let sql = format!("GRANT {privileges} ON {pattern}.* TO {account}");
        self.query(&sql).await?;
        Ok(())
    }

    async fn close(&self) {
        self.pool.close().await;
    }

    async fn cancel(&self) -> AppResult<()> {
        let id = self.server_id.load(Ordering::Relaxed);
        if id == 0 {
            return Ok(());
        }
        // The id is a number from the server, never text from the user.
        run_on_new_connection::<MySqlConnection>(&self.options, format!("KILL QUERY {id}")).await
    }
}
