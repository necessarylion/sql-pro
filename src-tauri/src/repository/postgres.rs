//! `PostgreSQL` access.

use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPool, PgQueryResult, PgRow};
use sqlx::{Postgres, Row as _};

use super::{
    Repository, cell, collect, default_sql, double_quote, first_column, kind, literal,
    pool_options, single, standard_columns,
};
use crate::error::AppResult;
use crate::model::{ColumnInfo, ColumnSpec, ConnectionConfig, ResultSet, TableRef};

const DATABASES_SQL: &str =
    "SELECT datname FROM pg_database WHERE NOT datistemplate AND datallowconn ORDER BY datname";

const TABLES_SQL: &str = "SELECT table_schema, table_name, table_type \
    FROM information_schema.tables \
    WHERE table_schema NOT IN ('pg_catalog', 'information_schema') \
    ORDER BY table_schema, table_name";

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

    async fn close(&self) {
        self.pool.close().await;
    }
}
