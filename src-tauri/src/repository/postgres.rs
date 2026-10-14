//! `PostgreSQL` access.

use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPool, PgQueryResult, PgRow};
use sqlx::{Postgres, Row as _};

use super::{
    Repository, Statement, cell, collect, default_sql, double_quote, execute_all, first_column,
    kind, literal, password_safe, pool_options, single, standard_columns,
};
use crate::constants::POSTGRES_ROLE_NAME_MAX;
use crate::error::{AppError, AppResult};
use crate::model::{
    AccessLevel, ColumnInfo, ColumnSpec, ConnectionConfig, DbUser, ResultSet, TableRef, UserList,
};

const DATABASES_SQL: &str =
    "SELECT datname FROM pg_database WHERE NOT datistemplate AND datallowconn ORDER BY datname";

const TABLES_SQL: &str = "SELECT table_schema, table_name, table_type \
    FROM information_schema.tables \
    WHERE table_schema NOT IN ('pg_catalog', 'information_schema') \
    ORDER BY table_schema, table_name";

/// The schemas that hold user data. Names that start with `pg_` belong to the system.
const USER_SCHEMAS_SQL: &str = "SELECT nspname FROM pg_namespace n \
    WHERE n.nspname <> 'information_schema' AND left(n.nspname, 3) <> 'pg_' ORDER BY 1";

const CAN_MANAGE_SQL: &str =
    "SELECT rolcreaterole OR rolsuper FROM pg_roles WHERE rolname = current_user";

/// Every role that can log in: name, access level, and whether it is the current role.
///
/// The level comes from the rights granted to the role itself, in this database:
/// - `admin`: a superuser, or the owner of the database (or a member of the owner).
/// - `none`: no grant on the database, its schemas, tables and sequences, and no default grant.
/// - `read`: exactly SELECT on every table and view. A default grant counts as one more table.
/// - `write`: exactly SELECT, INSERT, UPDATE and DELETE on every table and view.
/// - `custom`: everything else.
// ponytail: rights that come from PUBLIC or from a group role are not counted, and
// a table the role owns counts as "no grant". Use `has_table_privilege` if the real,
// effective rights must be shown.
// ponytail: this reads the grants of every table once per role. It gets slow with
// many thousand tables. Load the level per role on demand if that hurts.
const USERS_SQL: &str = "SELECT r.rolname, \
    CASE \
        WHEN r.rolsuper OR pg_has_role(r.oid, d.datdba, 'USAGE') THEN 'admin' \
        WHEN (t.sets IS NULL OR t.sets = ARRAY['']) AND NOT o.found THEN 'none' \
        WHEN t.sets = ARRAY['SELECT'] THEN 'read' \
        WHEN t.sets = ARRAY['DELETE,INSERT,SELECT,UPDATE'] THEN 'write' \
        ELSE 'custom' \
    END, \
    r.rolname = current_user \
    FROM pg_roles r \
    CROSS JOIN (SELECT datdba, datacl FROM pg_database WHERE datname = current_database()) d \
    CROSS JOIN LATERAL ( \
        SELECT array_agg(DISTINCT x.privileges ORDER BY x.privileges) AS sets FROM ( \
            SELECT (SELECT coalesce(string_agg(DISTINCT a.privilege_type, ',' \
                        ORDER BY a.privilege_type), '') \
                    FROM aclexplode(g.acl) a WHERE a.grantee = r.oid) AS privileges, g.is_default \
            FROM (SELECT c.relacl AS acl, false AS is_default \
                    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                    WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') \
                        AND n.nspname <> 'information_schema' AND left(n.nspname, 3) <> 'pg_' \
                UNION ALL \
                SELECT defaclacl, true FROM pg_default_acl WHERE defaclobjtype = 'r') g \
        ) x WHERE NOT (x.is_default AND x.privileges = '') \
    ) t \
    CROSS JOIN LATERAL ( \
        SELECT EXISTS (SELECT 1 FROM ( \
            SELECT d.datacl AS acl \
            UNION ALL \
            SELECT n.nspacl FROM pg_namespace n \
                WHERE n.nspname <> 'information_schema' AND left(n.nspname, 3) <> 'pg_' \
            UNION ALL \
            SELECT c.relacl FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE c.relkind = 'S' \
                    AND n.nspname <> 'information_schema' AND left(n.nspname, 3) <> 'pg_' \
            UNION ALL \
            SELECT defaclacl FROM pg_default_acl \
        ) s, aclexplode(s.acl) a WHERE a.grantee = r.oid) AS found \
    ) o \
    WHERE r.rolcanlogin AND left(r.rolname, 3) <> 'pg_' \
    ORDER BY r.rolname";

pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub async fn connect(config: &ConnectionConfig) -> AppResult<Self> {
        let mut options = PgConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .username(&config.user)
            .password(&config.password);
        if !config.database.is_empty() {
            options = options.database(&config.database);
        }
        let pool = pool_options().connect_with(options).await?;
        Ok(Self { pool })
    }

    /// Writes a text value as an escape string: `E'value'`.
    /// It reads the same with every `standard_conforming_strings` setting.
    fn text(value: &str) -> String {
        format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
    }

    /// The first cell of the first row, as text.
    async fn value(&self, sql: &str) -> AppResult<String> {
        let set = single(self.query(sql).await?);
        Ok(set.rows.first().map(|row| cell(row, 0)).unwrap_or_default())
    }

    /// The statements that take away everything `role` was granted in this database.
    /// `role` and `database` are already quoted.
    // ponytail: default grants made by another role, or made for one schema only,
    // stay. So do grants on functions and types. The list then shows `custom`.
    fn revoke_sql(role: &str, database: &str, schemas: &[String]) -> Vec<String> {
        let mut statements = vec![
            format!("ALTER DEFAULT PRIVILEGES REVOKE ALL ON TABLES FROM {role}"),
            format!("ALTER DEFAULT PRIVILEGES REVOKE ALL ON SEQUENCES FROM {role}"),
            format!("REVOKE ALL ON DATABASE {database} FROM {role}"),
        ];
        for schema in schemas {
            statements.push(format!(
                "REVOKE ALL ON ALL TABLES IN SCHEMA {schema} FROM {role}"
            ));
            statements.push(format!(
                "REVOKE ALL ON ALL SEQUENCES IN SCHEMA {schema} FROM {role}"
            ));
            statements.push(format!("REVOKE ALL ON SCHEMA {schema} FROM {role}"));
        }
        statements
    }

    /// The statements that let `role` read, or read and write, every table of this database.
    /// Tables that the current role creates later are covered by the default grants.
    // ponytail: a schema that is created later is not covered. Set the access again then.
    fn grant_sql(role: &str, database: &str, schemas: &[String], write: bool) -> Vec<String> {
        let (tables, sequences) = if write {
            // USAGE on a sequence is what an INSERT into a serial column needs.
            ("SELECT, INSERT, UPDATE, DELETE", "SELECT, USAGE")
        } else {
            ("SELECT", "SELECT")
        };
        let mut statements = vec![
            format!("GRANT CONNECT ON DATABASE {database} TO {role}"),
            format!("ALTER DEFAULT PRIVILEGES GRANT {tables} ON TABLES TO {role}"),
            format!("ALTER DEFAULT PRIVILEGES GRANT {sequences} ON SEQUENCES TO {role}"),
        ];
        for schema in schemas {
            statements.push(format!("GRANT USAGE ON SCHEMA {schema} TO {role}"));
            statements.push(format!(
                "GRANT {tables} ON ALL TABLES IN SCHEMA {schema} TO {role}"
            ));
            statements.push(format!(
                "GRANT {sequences} ON ALL SEQUENCES IN SCHEMA {schema} TO {role}"
            ));
        }
        statements
    }

    /// The quoted name of the current database, and the quoted names of its user schemas.
    async fn grant_targets(&self) -> AppResult<(String, Vec<String>)> {
        let database = self.quote(&self.value("SELECT current_database()").await?);
        let set = single(self.query(USER_SCHEMAS_SQL).await?);
        let mut schemas = Vec::with_capacity(set.rows.len());
        for name in first_column(&set) {
            schemas.push(self.quote(&name));
        }
        Ok((database, schemas))
    }

    /// Runs statements as one unit. The server rolls all of them back when one fails,
    /// because one message with many statements is one transaction.
    async fn run_all(&self, statements: &[String]) -> AppResult<()> {
        self.query(&statements.join("; ")).await?;
        Ok(())
    }

    /// Runs a statement that holds a password. The error never shows the password.
    async fn run_secret(&self, sql: &str, password: &str) -> AppResult<()> {
        match self.query(sql).await {
            Ok(_) => Ok(()),
            Err(error) => Err(password_safe(error, password)),
        }
    }

    /// `raw_sql` uses the simple query protocol, so the server sends every value as text.
    fn read(row: &PgRow, index: usize) -> Result<Option<String>, sqlx::Error> {
        row.try_get_unchecked(index)
    }
}

#[async_trait]
impl Repository for PostgresRepository {
    fn quote(&self, identifier: &str) -> String {
        double_quote(identifier)
    }

    /// We bind every value as text, so most types need a cast.
    /// Text types get no cast: an explicit cast to `varchar(n)` would cut long text silently.
    fn placeholder(&self, index: usize, data_type: &str) -> String {
        if data_type.is_empty() || data_type == "text" || data_type.starts_with("character") {
            format!("${index}")
        } else {
            format!("CAST(${index} AS {data_type})")
        }
    }

    fn literal(&self, value: &str) -> String {
        literal(value)
    }

    /// `PostgreSQL` has no LIKE for numbers and dates, so we compare the text form.
    fn like_operand(&self, column: &str) -> String {
        format!("CAST({column} AS TEXT)")
    }

    fn default_values(&self) -> &'static str {
        "DEFAULT VALUES"
    }

    // ponytail: identity columns need PostgreSQL 10. Use SERIAL if older servers must work.
    fn auto_increment(&self) -> &'static str {
        "GENERATED BY DEFAULT AS IDENTITY"
    }

    async fn query(&self, sql: &str) -> AppResult<Vec<ResultSet>> {
        let stream = sqlx::raw_sql(sql).fetch_many(&self.pool);
        collect::<Postgres>(stream, Self::read, PgQueryResult::rows_affected).await
    }

    async fn execute(&self, sql: &str, params: Vec<Option<String>>) -> AppResult<u64> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = query.bind(param);
        }
        Ok(query.execute(&self.pool).await?.rows_affected())
    }

    async fn execute_all(&self, statements: Vec<Statement>) -> AppResult<Vec<u64>> {
        execute_all(&self.pool, statements, PgQueryResult::rows_affected).await
    }

    async fn databases(&self) -> AppResult<Vec<String>> {
        Ok(first_column(&single(self.query(DATABASES_SQL).await?)))
    }

    async fn tables(&self) -> AppResult<Vec<TableRef>> {
        let set = single(self.query(TABLES_SQL).await?);
        let mut tables = Vec::with_capacity(set.rows.len());
        for row in &set.rows {
            tables.push(TableRef {
                schema: Some(cell(row, 0)),
                name: cell(row, 1),
                kind: kind(&cell(row, 2)),
            });
        }
        Ok(tables)
    }

    async fn columns(&self, table: &TableRef) -> AppResult<Vec<ColumnInfo>> {
        let sql = format!(
            "SELECT a.attname, format_type(a.atttypid, a.atttypmod), \
                CASE WHEN a.attnotnull THEN 'NO' ELSE 'YES' END, \
                pg_get_expr(d.adbin, d.adrelid), \
                CASE WHEN i.indisprimary THEN 'PRI' ELSE '' END, \
                CASE WHEN a.attidentity <> '' THEN 'identity' \
                     WHEN a.attgenerated <> '' THEN 'generated' ELSE '' END \
            FROM pg_attribute a \
            LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
            LEFT JOIN pg_index i ON i.indrelid = a.attrelid AND i.indisprimary \
                AND a.attnum = ANY(i.indkey) \
            WHERE a.attrelid = {}::regclass AND a.attnum > 0 AND NOT a.attisdropped \
            ORDER BY a.attnum",
            literal(&self.table_name(table))
        );
        Ok(standard_columns(&single(self.query(&sql).await?)))
    }

    async fn modify_column_sql(
        &self,
        table: &TableRef,
        old: &ColumnInfo,
        new: &ColumnSpec,
    ) -> AppResult<Vec<String>> {
        let table = self.table_name(table);
        let column = self.quote(&old.name);
        let mut statements = Vec::new();
        if new.data_type != old.data_type {
            let data_type = &new.data_type;
            statements.push(format!(
                "ALTER TABLE {table} ALTER COLUMN {column} TYPE {data_type} USING {column}::{data_type}"
            ));
        }
        if new.nullable != old.nullable {
            let action = if new.nullable {
                "DROP NOT NULL"
            } else {
                "SET NOT NULL"
            };
            statements.push(format!(
                "ALTER TABLE {table} ALTER COLUMN {column} {action}"
            ));
        }
        statements.extend(default_sql(&table, &column, old, new));
        Ok(statements)
    }

    async fn users(&self) -> AppResult<UserList> {
        let can_manage = self.value(CAN_MANAGE_SQL).await? == "t";
        let set = single(self.query(USERS_SQL).await?);
        let mut users = Vec::with_capacity(set.rows.len());
        for row in &set.rows {
            let access = match cell(row, 1).as_str() {
                "admin" => AccessLevel::Admin,
                "none" => AccessLevel::None,
                "read" => AccessLevel::Read,
                "write" => AccessLevel::Write,
                _ => AccessLevel::Custom,
            };
            users.push(DbUser {
                name: cell(row, 0),
                host: None,
                access,
                current: cell(row, 2) == "t",
            });
        }
        Ok(UserList { can_manage, users })
    }

    async fn create_user(&self, name: &str, _host: Option<&str>, password: &str) -> AppResult<()> {
        if name.len() > POSTGRES_ROLE_NAME_MAX {
            return Err(AppError::Message(format!(
                "The user name is too long. The limit is {POSTGRES_ROLE_NAME_MAX} bytes."
            )));
        }
        let sql = format!(
            "CREATE ROLE {} LOGIN PASSWORD {}",
            self.quote(name),
            Self::text(password)
        );
        self.run_secret(&sql, password).await
    }

    /// A role that still has rights cannot be dropped. So the rights in this database
    /// go first, in the same transaction. Nothing changes when the drop fails.
    // No DROP OWNED and no REASSIGN on purpose. When the role owns objects, or has
    // rights in another database, the database says no.
    async fn drop_user(&self, name: &str, _host: Option<&str>) -> AppResult<()> {
        let role = self.quote(name);
        let (database, schemas) = self.grant_targets().await?;
        let mut statements = Self::revoke_sql(&role, &database, &schemas);
        statements.push(format!("DROP ROLE {role}"));
        self.run_all(&statements).await
    }

    async fn set_user_password(
        &self,
        name: &str,
        _host: Option<&str>,
        password: &str,
    ) -> AppResult<()> {
        let sql = format!(
            "ALTER ROLE {} PASSWORD {}",
            self.quote(name),
            Self::text(password)
        );
        self.run_secret(&sql, password).await
    }

    async fn set_user_access(
        &self,
        name: &str,
        _host: Option<&str>,
        access: AccessLevel,
    ) -> AppResult<()> {
        let fixed_sql = format!(
            "SELECT r.rolsuper OR pg_has_role(r.oid, d.datdba, 'USAGE') \
            FROM pg_roles r, pg_database d \
            WHERE d.datname = current_database() AND r.rolname = {}",
            Self::text(name)
        );
        if self.value(&fixed_sql).await? == "t" {
            return Err(
                "This account is a superuser or owns the database. It can do everything \
                here, and a change of rights on one database cannot take that away."
                    .into(),
            );
        }
        let role = self.quote(name);
        let (database, schemas) = self.grant_targets().await?;
        let mut statements = Self::revoke_sql(&role, &database, &schemas);
        if access != AccessLevel::None {
            let write = access == AccessLevel::Write;
            statements.extend(Self::grant_sql(&role, &database, &schemas, write));
        }
        self.run_all(&statements).await
    }

    async fn close(&self) {
        self.pool.close().await;
    }
}
