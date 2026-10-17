//! Microsoft SQL Server access, through our fork of the `sqlx-sqlserver` driver.

use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Timelike as _};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use sqlx::error::BoxDynError;
use sqlx::{AssertSqlSafe, Decode, Row as _, ValueRef as _};
use sqlx_sqlserver::{
    Mssql, MssqlConnectOptions, MssqlConnection, MssqlPool, MssqlQueryResult, MssqlRow, MssqlType,
    MssqlValueRef,
};

use super::{
    Repository, ServerId, Statement, cell, collect, definition, execute, execute_all, first_column,
    foreign_keys, hex, indexes, kind, objects, pool_options, relations, run_on_new_connection,
    run_script, single, standard_columns,
};
use crate::constants::{MSSQL_NO_QUERY_STATS, QUERY_CANCELLED, QUERY_STATS_LIMIT};
use crate::error::{AppError, AppResult};
use crate::model::{
    ColumnInfo, ColumnSpec, ConnectionConfig, DbObject, ForeignKey, IndexInfo, Relation, ResultSet,
    SslMode, TableRef,
};

/// The schema a name without a schema lands in.
const DEFAULT_SCHEMA: &str = "dbo";

const DATABASES_SQL: &str =
    "SELECT name FROM sys.databases WHERE HAS_DBACCESS(name) = 1 ORDER BY name";

const TABLES_SQL: &str = "SELECT s.name, o.name, o.type_desc \
    FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
    WHERE o.type IN ('U', 'V') AND o.is_ms_shipped = 0 \
    ORDER BY s.name, o.name";

/// One row per foreign key column, in the shape `relations` reads.
const FOREIGN_KEYS_SQL: &str = "SELECT s.name, t.name, fk.object_id, fk.name, pc.name, \
        rs.name, rt.name, rc.name \
    FROM sys.foreign_keys fk \
    JOIN sys.tables t ON t.object_id = fk.parent_object_id \
    JOIN sys.schemas s ON s.schema_id = t.schema_id \
    JOIN sys.foreign_key_columns k ON k.constraint_object_id = fk.object_id \
    JOIN sys.columns pc ON pc.object_id = k.parent_object_id AND pc.column_id = k.parent_column_id \
    JOIN sys.tables rt ON rt.object_id = k.referenced_object_id \
    JOIN sys.schemas rs ON rs.schema_id = rt.schema_id \
    JOIN sys.columns rc ON rc.object_id = k.referenced_object_id \
        AND rc.column_id = k.referenced_column_id \
    ORDER BY s.name, t.name, fk.name, k.constraint_column_id";

/// One row per key column of every unique index. The primary key is one of them.
const UNIQUE_SQL: &str = "SELECT s.name, t.name, i.name, c.name \
    FROM sys.indexes i \
    JOIN sys.tables t ON t.object_id = i.object_id \
    JOIN sys.schemas s ON s.schema_id = t.schema_id \
    JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
    JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
    WHERE i.is_unique = 1 AND i.has_filter = 0 AND ic.is_included_column = 0";

/// The triggers, procedures and functions, in the shape `objects` reads.
const OBJECTS_SQL: &str = "SELECT 'trigger', s.name, tr.name, p.name, NULL \
    FROM sys.triggers tr \
    JOIN sys.objects p ON p.object_id = tr.parent_id \
    JOIN sys.schemas s ON s.schema_id = p.schema_id \
    WHERE tr.is_ms_shipped = 0 \
    UNION ALL \
    SELECT CASE WHEN o.type = 'P' THEN 'procedure' ELSE 'function' END, s.name, o.name, \
        NULL, NULL \
    FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id \
    WHERE o.type IN ('P', 'FN', 'IF', 'TF') AND o.is_ms_shipped = 0 \
    ORDER BY 1 DESC, 2, 3";

/// The column types the driver cannot read. Browse and export cast them to text.
const TEXT_CAST_TYPES: [&str; 5] = ["xml", "text", "ntext", "sql_variant", "image"];

/// The CLR types. They turn into text with their own `ToString()` method.
const CLR_TYPES: [&str; 3] = ["geography", "geometry", "hierarchyid"];

/// The binary column types. A value for them comes as `0x…` text and is read as hex.
const BINARY_TYPES: [&str; 4] = ["binary", "varbinary", "image", "timestamp"];

/// The start of the driver error for a column type it cannot read.
const UNSUPPORTED_TYPE: &str = "unsupported TDS data type ";

/// Where each byte of a `uniqueidentifier` goes in its text.
/// The first three groups are stored little-endian, the rest as it is.
const GUID_ORDER: [usize; 16] = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];

pub struct MssqlRepository {
    pool: MssqlPool,
    /// The settings of the pool. Cancel opens its own connection with them.
    options: MssqlConnectOptions,
    /// The session id (`@@SPID`) of the one connection of the pool.
    server_id: ServerId,
    /// A user query runs now. A cancel only acts then.
    running: AtomicBool,
    /// A cancel ended the session of the running query.
    cancelled: AtomicBool,
}

impl MssqlRepository {
    pub async fn connect(config: &ConnectionConfig) -> AppResult<Self> {
        let options = MssqlConnectOptions::parse_url(&Self::url(config))
            .map_err(|error| AppError::Message(format!("Bad connection settings: {error}")))?;
        let server_id = ServerId::default();
        let seen = Arc::clone(&server_id);
        let pool = pool_options()
            .after_connect(move |connection, _| {
                let seen = Arc::clone(&seen);
                Box::pin(async move {
                    let id: i64 = sqlx::query_scalar("SELECT CAST(@@SPID AS BIGINT)")
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
            options,
            server_id,
            running: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        })
    }

    /// The driver only reads its settings from a URL. Every part is escaped, so a
    /// password with `@` or `/` still works. The URL is never logged or shown.
    ///
    /// By default the traffic is encrypted, but the certificate of the server is not
    /// checked, because most servers have a self-signed certificate. `Prefer` and
    /// `Require` both mean that. `Off` encrypts only the login, like `Encrypt=false`
    /// in other SQL Server clients. The driver has no client certificate option.
    fn url(config: &ConnectionConfig) -> String {
        let host = if config.host.contains(':') {
            format!("[{}]", config.host)
        } else {
            config.host.clone()
        };
        let mut url = format!(
            "mssql://{}:{}@{host}:{}/",
            utf8_percent_encode(&config.user, NON_ALPHANUMERIC),
            utf8_percent_encode(&config.password, NON_ALPHANUMERIC),
            config.port
        );
        url.extend(utf8_percent_encode(&config.database, NON_ALPHANUMERIC));
        let ssl = &config.ssl;
        let (encrypt, trust) = match ssl.mode {
            SslMode::Off => ("optional", "true"),
            SslMode::Prefer | SslMode::Require => ("mandatory", "true"),
            SslMode::VerifyCa | SslMode::VerifyFull => ("mandatory", "false"),
        };
        url.push_str("?encrypt=");
        url.push_str(encrypt);
        url.push_str("&trust_server_certificate=");
        url.push_str(trust);
        if trust == "false" && !ssl.ca_path.is_empty() {
            url.push_str("&ssl-ca=");
            url.extend(utf8_percent_encode(&ssl.ca_path, NON_ALPHANUMERIC));
        }
        // The driver checks the host name only when this is set.
        if ssl.mode == SslMode::VerifyFull {
            let name = if ssl.server_name.is_empty() {
                &config.host
            } else {
                &ssl.server_name
            };
            url.push_str("&hostname_in_certificate=");
            url.extend(utf8_percent_encode(name, NON_ALPHANUMERIC));
        }
        url
    }

    /// The name of a table as a text for `OBJECT_ID`, like `N'[dbo].[t]'`.
    fn object_id(&self, table: &TableRef) -> String {
        format!("OBJECT_ID({})", self.literal(&self.table_name(table)))
    }

    /// SQL that drops the default of a column, when it has one.
    /// A default is a constraint with its own name, so the name is looked up first.
    fn drop_default_sql(&self, table: &TableRef, column: &str) -> String {
        let object = self.object_id(table);
        let drop = self.literal(&format!(
            "ALTER TABLE {} DROP CONSTRAINT ",
            self.table_name(table)
        ));
        format!(
            "DECLARE @sqlpro_drop nvarchar(max) = (SELECT {drop} + QUOTENAME(d.name) \
                FROM sys.default_constraints d WHERE d.parent_object_id = {object} \
                AND d.parent_column_id = COLUMNPROPERTY({object}, {}, 'ColumnId')); \
            IF @sqlpro_drop IS NOT NULL EXEC (@sqlpro_drop)",
            self.literal(column)
        )
    }

    /// Turns one cell into text, the way SQL Server writes the value. NULL stays `None`.
    fn read(row: &MssqlRow, index: usize) -> Result<Option<String>, sqlx::Error> {
        let value = row.try_get_raw(index)?;
        if value.is_null() {
            return Ok(None);
        }
        Self::text(value).map(Some).map_err(sqlx::Error::Decode)
    }

    /// The driver cannot read some column types, like `xml`. Its error then says
    /// what to do instead. Browsing a table casts such columns by itself.
    fn readable(error: AppError) -> AppError {
        let AppError::Database(sqlx::Error::Protocol(message)) = &error else {
            return error;
        };
        let Some((_, name)) = message.split_once(UNSUPPORTED_TYPE) else {
            return error;
        };
        AppError::Message(format!(
            "The result has a column of type {name}. The app cannot read this type yet. \
            Cast the column to text in the query, like CAST(column AS NVARCHAR(MAX))."
        ))
    }

    /// Decodes a value as the SQL Server type `T`.
    fn get<'r, T: Decode<'r, Mssql>>(value: MssqlValueRef<'r>) -> Result<T, BoxDynError> {
        T::decode(value)
    }

    fn text(value: MssqlValueRef<'_>) -> Result<String, BoxDynError> {
        let info = value.type_info().into_owned();
        Ok(match info.kind() {
            MssqlType::Null => String::new(),
            MssqlType::Bit => if Self::get::<bool>(value)? { "1" } else { "0" }.to_owned(),
            MssqlType::TinyInt | MssqlType::SmallInt | MssqlType::Int | MssqlType::BigInt => {
                Self::get::<i64>(value)?.to_string()
            }
            MssqlType::Real => Self::get::<f32>(value)?.to_string(),
            MssqlType::Float => Self::get::<f64>(value)?.to_string(),
            MssqlType::Decimal => Self::decimal(Self::get::<&[u8]>(value)?, info.scale())?,
            MssqlType::Money => Self::money(Self::get::<&[u8]>(value)?)?,
            MssqlType::VarChar => Self::single_byte_text(Self::get::<&[u8]>(value)?),
            MssqlType::NVarChar => Self::get::<String>(value)?,
            MssqlType::VarBinary => hex(Self::get::<&[u8]>(value)?),
            MssqlType::UniqueIdentifier => Self::guid(Self::get::<&[u8]>(value)?)?,
            MssqlType::Date => Self::get::<NaiveDate>(value)?
                .format("%Y-%m-%d")
                .to_string(),
            MssqlType::Time => Self::clock(Self::get::<NaiveTime>(value)?, info.scale()),
            MssqlType::DateTime2 => Self::moment(Self::get::<NaiveDateTime>(value)?, info.scale()),
            // A `smalldatetime` (4 bytes) has no fraction. A `datetime` has 3 digits.
            MssqlType::DateTime => {
                let digits = if Self::get::<&[u8]>(value)?.len() == 4 {
                    0
                } else {
                    3
                };
                Self::moment(Self::get::<NaiveDateTime>(value)?, digits)
            }
            MssqlType::DateTimeOffset => {
                let moment = Self::get::<DateTime<FixedOffset>>(value)?;
                let local = Self::moment(moment.naive_local(), info.scale());
                format!("{local} {}", moment.format("%:z"))
            }
            MssqlType::Other(_) => match Self::get::<String>(value) {
                Ok(text) => text,
                Err(_) => hex(Self::get::<&[u8]>(value)?),
            },
        })
    }

    /// A time of day with `digits` digits after the seconds, like SQL Server writes it.
    fn clock(time: NaiveTime, digits: u8) -> String {
        let mut text = time.format("%H:%M:%S").to_string();
        if digits > 0 {
            let nanos = format!("{:09}", time.nanosecond());
            text.push('.');
            text.push_str(&nanos[..usize::from(digits.min(9))]);
        }
        text
    }

    /// A date and time with `digits` digits after the seconds.
    fn moment(moment: NaiveDateTime, digits: u8) -> String {
        format!(
            "{} {}",
            moment.format("%Y-%m-%d"),
            Self::clock(moment.time(), digits)
        )
    }

    /// A `varchar` value. It is UTF-8 with a UTF-8 collation, else one byte per letter.
    // ponytail: a one byte value is read as Latin-1. Letters of other code pages
    // (Cyrillic, Greek, ...) come out wrong. Use the collation of the column if that hurts.
    fn single_byte_text(bytes: &[u8]) -> String {
        match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => bytes.iter().map(|byte| char::from(*byte)).collect(),
        }
    }

    /// A `decimal` or `numeric`: a sign byte, then the digits as a little-endian number.
    fn decimal(bytes: &[u8], scale: u8) -> Result<String, BoxDynError> {
        let Some((&sign, digits)) = bytes.split_first() else {
            return Err("empty decimal value".into());
        };
        if digits.len() > 16 {
            return Err("decimal value is too long".into());
        }
        let mut fixed = [0_u8; 16];
        fixed[..digits.len()].copy_from_slice(digits);
        Ok(Self::fixed_point(
            sign == 0,
            u128::from_le_bytes(fixed),
            scale,
        ))
    }

    /// A `money` (8 bytes, high half first) or `smallmoney` (4 bytes), in units of 1/10000.
    fn money(bytes: &[u8]) -> Result<String, BoxDynError> {
        let amount = match bytes.len() {
            4 => i64::from(i32::from_le_bytes(bytes.try_into()?)),
            8 => {
                let (high, low) = bytes.split_at(4);
                (i64::from(i32::from_le_bytes(high.try_into()?)) << 32)
                    | i64::from(u32::from_le_bytes(low.try_into()?))
            }
            _ => return Err("money value has a wrong length".into()),
        };
        Ok(Self::fixed_point(
            amount < 0,
            u128::from(amount.unsigned_abs()),
            4,
        ))
    }

    /// Writes `magnitude` with `scale` digits after the point, like `-12.50`.
    fn fixed_point(negative: bool, magnitude: u128, scale: u8) -> String {
        let scale = usize::from(scale);
        let digits = format!("{magnitude:0>width$}", width = scale + 1);
        let (whole, fraction) = digits.split_at(digits.len() - scale);
        let sign = if negative && magnitude != 0 { "-" } else { "" };
        if fraction.is_empty() {
            format!("{sign}{whole}")
        } else {
            format!("{sign}{whole}.{fraction}")
        }
    }

    /// A `uniqueidentifier`. The first three groups are stored little-endian.
    fn guid(bytes: &[u8]) -> Result<String, BoxDynError> {
        let Ok(bytes) = <&[u8; 16]>::try_from(bytes) else {
            return Err("uniqueidentifier value has a wrong length".into());
        };
        let mut text = String::with_capacity(36);
        for (position, index) in GUID_ORDER.iter().enumerate() {
            if matches!(position, 4 | 6 | 8 | 10) {
                text.push('-');
            }
            let _ = write!(text, "{:02X}", bytes[*index]);
        }
        Ok(text)
    }

    /// The type name without its size, in lower case: `nvarchar(50)` gives `nvarchar`.
    fn base_type(data_type: &str) -> String {
        let end = data_type.find('(').unwrap_or(data_type.len());
        data_type[..end].trim().to_ascii_lowercase()
    }
}

#[async_trait]
impl Repository for MssqlRepository {
    fn quote(&self, identifier: &str) -> String {
        format!("[{}]", identifier.replace(']', "]]"))
    }

    /// Values are bound as `nvarchar`. SQL Server turns that into the column type
    /// by itself, except for binary types. Those get the text `0x…` and read it as hex.
    fn placeholder(&self, index: usize, data_type: &str) -> String {
        if BINARY_TYPES.contains(&Self::base_type(data_type).as_str()) {
            format!("CONVERT(VARBINARY(MAX), @p{index}, 1)")
        } else {
            format!("@p{index}")
        }
    }

    /// An `N'…'` text keeps every Unicode letter. A backslash is plain text here.
    fn literal(&self, value: &str) -> String {
        format!("N'{}'", value.replace('\'', "''"))
    }

    fn like_operand(&self, column: &str) -> String {
        format!("CAST({column} AS NVARCHAR(MAX))")
    }

    fn default_values(&self) -> &'static str {
        "DEFAULT VALUES"
    }

    /// SQL Server has no read-only transaction. The transaction is always rolled back,
    /// so a write inside it does not stay.
    fn read_only_guard(&self) -> (&'static str, &'static str) {
        ("BEGIN TRAN", "ROLLBACK")
    }

    fn open_transaction_sql(&self) -> Option<&'static str> {
        Some("SELECT @@TRANCOUNT")
    }

    fn auto_increment(&self) -> &'static str {
        "IDENTITY(1,1)"
    }

    fn drop_index_sql(&self, table: &TableRef, name: &str) -> String {
        format!(
            "DROP INDEX {} ON {}",
            self.quote(name),
            self.table_name(table)
        )
    }

    fn rename_table_sql(&self, table: &TableRef, new_name: &str) -> String {
        format!(
            "EXEC sp_rename {}, {}",
            self.literal(&self.table_name(table)),
            self.literal(new_name)
        )
    }

    fn rename_column_sql(&self, table: &TableRef, old: &str, new: &str) -> String {
        let column = format!("{}.{}", self.table_name(table), self.quote(old));
        format!(
            "EXEC sp_rename {}, {}, 'COLUMN'",
            self.literal(&column),
            self.literal(new)
        )
    }

    fn add_column_sql(&self, table: &str, definition: &str) -> String {
        format!("ALTER TABLE {table} ADD {definition}")
    }

    /// A column with a default cannot be dropped. So the default goes first.
    fn drop_column_sql(&self, table: &TableRef, column: &str) -> String {
        format!(
            "{}; ALTER TABLE {} DROP COLUMN {}",
            self.drop_default_sql(table, column),
            self.table_name(table),
            self.quote(column)
        )
    }

    /// The driver cannot read some types. Those are turned into text on the server.
    fn select_column(&self, column: &ColumnInfo) -> String {
        let name = self.quote(&column.name);
        let base = Self::base_type(&column.data_type);
        match base.as_str() {
            "image" => format!("CAST({name} AS VARBINARY(MAX)) AS {name}"),
            // A `sql_variant` cannot become a MAX type.
            "sql_variant" => format!("CAST({name} AS NVARCHAR(4000)) AS {name}"),
            _ if TEXT_CAST_TYPES.contains(&base.as_str()) => {
                format!("CAST({name} AS NVARCHAR(MAX)) AS {name}")
            }
            _ if CLR_TYPES.contains(&base.as_str()) => format!("{name}.ToString() AS {name}"),
            _ => name,
        }
    }

    /// SQL Server pages with `OFFSET … FETCH`, and that needs an ORDER BY.
    fn page_sql(&self, ordering: &str, limit: usize, offset: u64) -> String {
        let ordering = if ordering.is_empty() {
            " ORDER BY (SELECT NULL)"
        } else {
            ordering
        };
        format!("{ordering} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY")
    }

    /// Runs on the one connection of the pool. When a cancel ended its session,
    /// that connection is let go, so the next query opens a new one.
    async fn query(&self, sql: &str) -> AppResult<Vec<ResultSet>> {
        let mut connection = self.pool.acquire().await?;
        self.cancelled.store(false, Ordering::SeqCst);
        self.running.store(true, Ordering::SeqCst);
        let stream = sqlx::raw_sql(AssertSqlSafe(sql.to_owned())).fetch_many(&mut *connection);
        let result = collect::<Mssql>(stream, Self::read, MssqlQueryResult::rows_affected).await;
        self.running.store(false, Ordering::SeqCst);
        if self.cancelled.swap(false, Ordering::SeqCst) && result.is_err() {
            drop(connection.detach());
            return Err(QUERY_CANCELLED.into());
        }
        result.map_err(Self::readable)
    }

    async fn execute(&self, sql: &str, params: Vec<Option<String>>) -> AppResult<u64> {
        execute(&self.pool, sql, params, MssqlQueryResult::rows_affected).await
    }

    async fn execute_all(&self, statements: Vec<Statement>) -> AppResult<Vec<u64>> {
        execute_all(&self.pool, statements, MssqlQueryResult::rows_affected).await
    }

    async fn run_script(&self, statements: Vec<Statement>) -> AppResult<()> {
        run_script(&self.pool, statements).await
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

    /// The type is written with its size, like `nvarchar(50)` or `decimal(10,2)`.
    async fn columns(&self, table: &TableRef) -> AppResult<Vec<ColumnInfo>> {
        let sql = format!(
            "SELECT c.name, \
                CASE \
                    WHEN t.name IN ('varchar', 'char', 'varbinary', 'binary') THEN t.name + '(' + \
                        CASE WHEN c.max_length = -1 THEN 'max' \
                            ELSE CAST(c.max_length AS varchar(10)) END + ')' \
                    WHEN t.name IN ('nvarchar', 'nchar') THEN t.name + '(' + \
                        CASE WHEN c.max_length = -1 THEN 'max' \
                            ELSE CAST(c.max_length / 2 AS varchar(10)) END + ')' \
                    WHEN t.name IN ('decimal', 'numeric') THEN t.name + '(' + \
                        CAST(c.precision AS varchar(10)) + ',' + CAST(c.scale AS varchar(10)) + ')' \
                    WHEN t.name IN ('datetime2', 'time', 'datetimeoffset') THEN t.name + '(' + \
                        CAST(c.scale AS varchar(10)) + ')' \
                    ELSE t.name \
                END, \
                CASE WHEN c.is_nullable = 1 THEN 'YES' ELSE 'NO' END, \
                d.definition, \
                CASE WHEN EXISTS (SELECT 1 FROM sys.indexes i \
                    JOIN sys.index_columns ic ON ic.object_id = i.object_id \
                        AND ic.index_id = i.index_id \
                    WHERE i.object_id = c.object_id AND i.is_primary_key = 1 \
                        AND ic.column_id = c.column_id) THEN 'PRI' ELSE '' END, \
                CASE WHEN c.is_identity = 1 THEN 'identity' \
                    WHEN c.is_computed = 1 THEN 'generated' ELSE '' END \
            FROM sys.columns c \
            JOIN sys.types t ON t.user_type_id = c.user_type_id \
            LEFT JOIN sys.default_constraints d ON d.object_id = c.default_object_id \
            WHERE c.object_id = {} \
            ORDER BY c.column_id",
            self.object_id(table)
        );
        Ok(standard_columns(&single(self.query(&sql).await?)))
    }

    async fn relations(&self) -> AppResult<Vec<Relation>> {
        let keys = single(self.query(FOREIGN_KEYS_SQL).await?);
        let unique = single(self.query(UNIQUE_SQL).await?);
        Ok(relations(&keys, &unique))
    }

    async fn indexes(&self, table: &TableRef) -> AppResult<Vec<IndexInfo>> {
        let sql = format!(
            "SELECT i.name, c.name, CASE WHEN i.is_unique = 1 THEN 'YES' ELSE 'NO' END, \
                CASE WHEN i.is_primary_key = 1 THEN 'YES' ELSE 'NO' END \
            FROM sys.indexes i \
            JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
            JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
            WHERE i.object_id = {} AND ic.is_included_column = 0 \
            ORDER BY i.is_primary_key DESC, i.name, ic.key_ordinal",
            self.object_id(table)
        );
        Ok(indexes(&single(self.query(&sql).await?)))
    }

    async fn foreign_keys(&self, table: &TableRef) -> AppResult<Vec<ForeignKey>> {
        let schema = table.schema.as_deref().unwrap_or(DEFAULT_SCHEMA);
        let sql = format!(
            "SELECT fk.object_id, fk.name, pc.name, \
                CASE WHEN rs.name <> {schema} THEN rs.name END, rt.name, rc.name, \
                REPLACE(fk.delete_referential_action_desc, '_', ' '), \
                REPLACE(fk.update_referential_action_desc, '_', ' ') \
            FROM sys.foreign_keys fk \
            JOIN sys.foreign_key_columns k ON k.constraint_object_id = fk.object_id \
            JOIN sys.columns pc ON pc.object_id = k.parent_object_id \
                AND pc.column_id = k.parent_column_id \
            JOIN sys.tables rt ON rt.object_id = k.referenced_object_id \
            JOIN sys.schemas rs ON rs.schema_id = rt.schema_id \
            JOIN sys.columns rc ON rc.object_id = k.referenced_object_id \
                AND rc.column_id = k.referenced_column_id \
            WHERE fk.parent_object_id = {table} \
            ORDER BY fk.name, fk.object_id, k.constraint_column_id",
            schema = self.literal(schema),
            table = self.object_id(table),
        );
        Ok(foreign_keys(&single(self.query(&sql).await?)))
    }

    /// A view gives its `CREATE VIEW`. A table gives `None`, so the caller builds it.
    async fn create_sql(&self, table: &TableRef) -> AppResult<Option<String>> {
        let sql = format!(
            "SELECT OBJECT_DEFINITION(v.object_id) FROM sys.views v WHERE v.object_id = {}",
            self.object_id(table)
        );
        let set = single(self.query(&sql).await?);
        Ok(set
            .rows
            .first()
            .and_then(|row| row.first().cloned().flatten()))
    }

    async fn objects(&self) -> AppResult<Vec<DbObject>> {
        Ok(objects(&single(self.query(OBJECTS_SQL).await?)))
    }

    /// Triggers, procedures and functions all live in the schema, so one lookup fits all.
    async fn object_sql(&self, object: &DbObject) -> AppResult<String> {
        let sql = format!(
            "SELECT OBJECT_DEFINITION(OBJECT_ID({}))",
            self.literal(&self.object_name(object))
        );
        definition(&single(self.query(&sql).await?), 0, &object.name)
    }

    /// `ALTER COLUMN` needs the full type and null rule. A default is its own
    /// constraint. It blocks a change of the type, so it is dropped first and added again.
    async fn modify_column_sql(
        &self,
        table: &TableRef,
        old: &ColumnInfo,
        new: &ColumnSpec,
    ) -> AppResult<Vec<String>> {
        let table_sql = self.table_name(table);
        let column = self.quote(&old.name);
        let type_changed = new.data_type != old.data_type;
        let default_changed = new.default != old.default;
        let drop_default = old.default.is_some() && (type_changed || default_changed);
        let mut statements = Vec::new();
        if drop_default {
            statements.push(self.drop_default_sql(table, &old.name));
        }
        if type_changed || new.nullable != old.nullable {
            let null = if new.nullable { "NULL" } else { "NOT NULL" };
            statements.push(format!(
                "ALTER TABLE {table_sql} ALTER COLUMN {column} {} {null}",
                new.data_type
            ));
        }
        if let Some(expression) = &new.default
            && (default_changed || drop_default)
        {
            statements.push(format!(
                "ALTER TABLE {table_sql} ADD DEFAULT {expression} FOR {column}"
            ));
        }
        Ok(statements)
    }

    /// User sessions, with the request and the SQL text that run now.
    /// Without the VIEW SERVER STATE right SQL Server shows only the own session.
    /// A session that never ran a request has 1900-01-01 as its start. That gives no
    /// time, because `DATEDIFF` in seconds overflows after 68 years.
    fn processes_sql(&self) -> AppResult<&'static str> {
        Ok("SELECT s.session_id, s.login_name, s.host_name, \
            DB_NAME(COALESCE(r.database_id, s.database_id)), COALESCE(r.status, s.status), \
            DATEDIFF(SECOND, NULLIF(COALESCE(r.start_time, s.last_request_start_time), \
            '19000101'), GETDATE()), \
            t.text, CASE WHEN s.session_id = @@SPID THEN 1 ELSE 0 END \
            FROM sys.dm_exec_sessions s \
            LEFT JOIN sys.dm_exec_requests r ON r.session_id = s.session_id \
            OUTER APPLY sys.dm_exec_sql_text(r.sql_handle) t \
            WHERE s.is_user_process = 1 ORDER BY s.session_id")
    }

    /// SQL Server cannot stop only the statement from outside. `KILL` ends the session.
    fn kill_sql(&self, id: i64, query_only: bool) -> AppResult<String> {
        if query_only {
            return Err("SQL Server can only end the whole session".into());
        }
        Ok(format!("KILL {id}"))
    }

    /// One row per `query_hash`. The offsets cut the statement out of its batch; they
    /// count bytes of UTF-16 text, so they are halved. Ad hoc SQL has no `dbid` in
    /// `dm_exec_sql_text`, so the database then comes from the plan. Times are in
    /// microseconds: / 1000 gives milliseconds. Only the database of this connection.
    fn query_stats_sql(&self, days: u32) -> AppResult<String> {
        Ok(format!(
            "SELECT TOP ({QUERY_STATS_LIMIT}) MAX(x.statement), MAX(x.db), SUM(x.execution_count), \
            CONVERT(nvarchar(30), MAX(x.last_execution_time), 120), \
            SUM(x.total_elapsed_time) / 1000.0, \
            SUM(x.total_elapsed_time) / 1000.0 / NULLIF(SUM(x.execution_count), 0), \
            MAX(x.max_elapsed_time) / 1000.0, SUM(x.total_rows) \
            FROM (SELECT s.query_hash, s.execution_count, s.last_execution_time, \
            s.total_elapsed_time, s.max_elapsed_time, s.total_rows, \
            DB_NAME(COALESCE(t.dbid, CONVERT(int, a.value))) AS db, \
            CONVERT(nvarchar(max), SUBSTRING(t.text, s.statement_start_offset / 2 + 1, \
            (CASE s.statement_end_offset WHEN -1 THEN DATALENGTH(t.text) \
            ELSE s.statement_end_offset END - s.statement_start_offset) / 2 + 1)) AS statement \
            FROM sys.dm_exec_query_stats s \
            CROSS APPLY sys.dm_exec_sql_text(s.sql_handle) t \
            OUTER APPLY (SELECT value FROM sys.dm_exec_plan_attributes(s.plan_handle) \
            WHERE attribute = 'dbid') a \
            WHERE s.last_execution_time >= DATEADD(DAY, -{days}, GETDATE()) \
            AND COALESCE(t.dbid, CONVERT(int, a.value)) = DB_ID()) x \
            GROUP BY x.query_hash ORDER BY SUM(x.total_elapsed_time) DESC"
        ))
    }

    fn query_stats_setup(&self) -> &'static str {
        MSSQL_NO_QUERY_STATS
    }

    async fn close(&self) {
        self.pool.close().await;
    }

    /// The driver has no cancel of its own. So a second connection ends the session
    /// of the tab with `KILL`. The tab then opens a new connection for its next query.
    /// Only user queries can be cancelled. A cancel while none runs does nothing.
    // ponytail: KILL ends the whole session: an open transaction is rolled back and
    // temp tables are gone. It also needs the ALTER ANY CONNECTION right. Send a TDS
    // attention packet from the driver if that is not good enough.
    async fn cancel(&self) -> AppResult<()> {
        let id = self.server_id.load(Ordering::Relaxed);
        if id == 0 || !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.cancelled.store(true, Ordering::SeqCst);
        // The id is a number from the server, never text from the user.
        run_on_new_connection::<MssqlConnection>(&self.options, format!("KILL {id}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::{MssqlRepository, SslMode};

    #[test]
    fn decimals_keep_their_scale() {
        assert_eq!(MssqlRepository::fixed_point(false, 1250, 2), "12.50");
        assert_eq!(MssqlRepository::fixed_point(true, 5, 3), "-0.005");
        assert_eq!(MssqlRepository::fixed_point(false, 42, 0), "42");
        assert_eq!(MssqlRepository::fixed_point(true, 0, 2), "0.00");
    }

    #[test]
    fn money_is_read_high_half_first() {
        // -1.5 is -15000 units.
        let bytes = (-15000_i64).to_le_bytes();
        let stored = [&bytes[4..], &bytes[..4]].concat();
        assert_eq!(MssqlRepository::money(&stored).unwrap(), "-1.5000");
        assert_eq!(
            MssqlRepository::money(&123_400_i32.to_le_bytes()).unwrap(),
            "12.3400"
        );
    }

    #[test]
    fn guids_put_the_first_groups_in_order() {
        let bytes = [
            0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        assert_eq!(
            MssqlRepository::guid(&bytes).unwrap(),
            "00112233-4455-6677-8899-AABBCCDDEEFF"
        );
    }

    #[test]
    fn the_url_escapes_every_part() {
        let config = crate::model::ConnectionConfig {
            driver: crate::model::Driver::Mssql,
            host: "::1".to_owned(),
            port: 1433,
            user: "sa".to_owned(),
            password: "p@ss/w:rd?#".to_owned(),
            database: "my db".to_owned(),
            ssh: None,
            ssl: crate::model::SslConfig::default(),
        };
        let url = MssqlRepository::url(&config);
        let options = sqlx_sqlserver::MssqlConnectOptions::parse_url(&url).expect("url");
        assert_eq!(options.password(), Some("p@ss/w:rd?#"));
        assert_eq!(options.database(), "my db");
        assert_eq!(options.host(), "[::1]");
    }

    fn parse(config: &crate::model::ConnectionConfig) -> sqlx_sqlserver::MssqlConnectOptions {
        sqlx_sqlserver::MssqlConnectOptions::parse_url(&MssqlRepository::url(config)).expect("url")
    }

    #[test]
    fn the_url_checks_the_certificate_only_when_asked() {
        let mut config = crate::model::ConnectionConfig {
            driver: crate::model::Driver::Mssql,
            host: "127.0.0.1".to_owned(),
            port: 1433,
            user: "sa".to_owned(),
            password: String::new(),
            database: String::new(),
            ssh: None,
            ssl: crate::model::SslConfig::default(),
        };
        let options = parse(&config);
        assert!(options.trust_server_certificate());
        assert_eq!(options.hostname_in_certificate(), None);

        config.ssl.mode = SslMode::VerifyFull;
        config.ssl.ca_path = "/certs/my ca.pem".to_owned();
        config.ssl.server_name = "db.example.com".to_owned();
        let options = parse(&config);
        assert!(!options.trust_server_certificate());
        assert_eq!(options.hostname_in_certificate(), Some("db.example.com"));
        assert_eq!(
            options.ssl_root_cert(),
            Some(std::path::Path::new("/certs/my ca.pem"))
        );
    }
}
