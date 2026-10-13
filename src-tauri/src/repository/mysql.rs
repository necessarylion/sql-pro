//! `MySQL` and `MariaDB` access.

use async_trait::async_trait;
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlQueryResult, MySqlRow};
use sqlx::{MySql, Row as _};

use super::{
    Repository, bytes_to_text, cell, collect, default_sql, first_column, kind, pool_options, single,
};
use crate::error::{AppError, AppResult};
use crate::model::{ColumnInfo, ColumnSpec, ConnectionConfig, ResultSet, TableRef};

pub struct MysqlRepository {
    pool: MySqlPool,
    has_database: bool,
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
        let pool = pool_options().connect_with(options).await?;
        Ok(Self { pool, has_database })
    }

    /// The definition of a column as `SHOW CREATE TABLE` prints it, without the name.
    /// Example: ``varchar(20) NOT NULL DEFAULT 'x' COMMENT 'note'``.
    async fn column_definition(&self, table: &TableRef, column: &str) -> AppResult<String> {
        let sql = format!("SHOW CREATE TABLE {}", self.table_name(table));
        let set = single(self.query(&sql).await?);
        let create = set.rows.first().map(|row| cell(row, 1)).unwrap_or_default();
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

    async fn query(&self, sql: &str) -> AppResult<Vec<ResultSet>> {
        let stream = sqlx::raw_sql(sql).fetch_many(&self.pool);
        collect::<MySql>(stream, Self::read, MySqlQueryResult::rows_affected).await
    }

    async fn execute(&self, sql: &str, params: Vec<Option<String>>) -> AppResult<u64> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = query.bind(param);
        }
        Ok(query.execute(&self.pool).await?.rows_affected())
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

    async fn close(&self) {
        self.pool.close().await;
    }
}
