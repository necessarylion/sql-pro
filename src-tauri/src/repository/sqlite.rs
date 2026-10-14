//! `SQLite` access.

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqliteQueryResult, SqliteRow};
use sqlx::{Row as _, Sqlite, TypeInfo as _, ValueRef as _};

use super::{
    Repository, Statement, bytes_to_text, cell, collect, double_quote, execute_all, kind, literal,
    pool_options, single, standard_columns,
};
use crate::error::AppResult;
use crate::model::{ColumnInfo, ColumnSpec, ConnectionConfig, ResultSet, TableRef};

const TABLES_SQL: &str = "SELECT name, type FROM sqlite_master \
    WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' \
    ORDER BY name";

pub struct SqliteRepository {
    pool: SqlitePool,
}

impl SqliteRepository {
    /// Opens an existing file. We never create one, so a typo does not leave junk files.
    pub async fn connect(config: &ConnectionConfig) -> AppResult<Self> {
        let options = SqliteConnectOptions::new().filename(&config.database);
        let pool = pool_options().connect_with(options).await?;
        Ok(Self { pool })
    }

    /// `SQLite` types each value on its own, so we look at the value, not the column.
    fn read(row: &SqliteRow, index: usize) -> Result<Option<String>, sqlx::Error> {
        let value = row.try_get_raw(index)?;
        if value.is_null() {
            return Ok(None);
        }
        let text = match value.type_info().name() {
            "INTEGER" => row.try_get_unchecked::<i64, _>(index)?.to_string(),
            "REAL" => format!("{:?}", row.try_get_unchecked::<f64, _>(index)?),
            "BLOB" => bytes_to_text(row.try_get_unchecked::<&[u8], _>(index)?),
            _ => row.try_get_unchecked::<String, _>(index)?,
        };
        Ok(Some(text))
    }
}

#[async_trait]
impl Repository for SqliteRepository {
    fn quote(&self, identifier: &str) -> String {
        double_quote(identifier)
    }

    fn placeholder(&self, _index: usize, _data_type: &str) -> String {
        "?".to_owned()
    }

    fn literal(&self, value: &str) -> String {
        literal(value)
    }

    fn default_values(&self) -> &'static str {
        "DEFAULT VALUES"
    }

    /// `SQLite` has no TRUNCATE. A DELETE without WHERE does the same.
    fn truncate_sql(&self, table: &str) -> String {
        format!("DELETE FROM {table}")
    }

    fn auto_increment(&self) -> &'static str {
        "AUTOINCREMENT"
    }

    async fn query(&self, sql: &str) -> AppResult<Vec<ResultSet>> {
        let stream = sqlx::raw_sql(sql).fetch_many(&self.pool);
        collect::<Sqlite>(stream, Self::read, SqliteQueryResult::rows_affected).await
    }

    async fn execute(&self, sql: &str, params: Vec<Option<String>>) -> AppResult<u64> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = query.bind(param);
        }
        Ok(query.execute(&self.pool).await?.rows_affected())
    }

    async fn execute_all(&self, statements: Vec<Statement>) -> AppResult<Vec<u64>> {
        execute_all(&self.pool, statements, SqliteQueryResult::rows_affected).await
    }

    /// A `SQLite` file is one database.
    async fn databases(&self) -> AppResult<Vec<String>> {
        Ok(Vec::new())
    }

    async fn tables(&self) -> AppResult<Vec<TableRef>> {
        let set = single(self.query(TABLES_SQL).await?);
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
        let sql = format!(
            "SELECT name, type, CASE WHEN \"notnull\" THEN 'NO' ELSE 'YES' END, dflt_value, \
                CASE WHEN pk > 0 THEN 'PRI' ELSE '' END, '' \
            FROM pragma_table_info({}) ORDER BY cid",
            literal(&table.name)
        );
        Ok(standard_columns(&single(self.query(&sql).await?)))
    }

    /// `SQLite` can only rename, add and drop columns. Other changes need a table rebuild.
    async fn modify_column_sql(
        &self,
        _table: &TableRef,
        old: &ColumnInfo,
        new: &ColumnSpec,
    ) -> AppResult<Vec<String>> {
        let same = new.data_type == old.data_type
            && new.nullable == old.nullable
            && new.default == old.default;
        if same {
            return Ok(Vec::new());
        }
        Err(
            "SQLite cannot change the type, null rule or default of a column. \
            It can only rename, add and drop columns."
                .into(),
        )
    }

    async fn close(&self) {
        self.pool.close().await;
    }
}
