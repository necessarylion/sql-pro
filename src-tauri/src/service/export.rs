//! Writes tables to a file: as SQL (`CREATE` and `INSERT` statements), CSV, JSON or Excel.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use rust_xlsxwriter::{ColNum, Format, RowNum, Workbook, Worksheet};

use crate::constants::MAX_QUERY_ROWS;
use crate::error::{AppError, AppResult};
use crate::model::{ColumnInfo, ExportFormat, ExportRequest, RowsExport, TableRef};
use crate::repository::Repository;

use super::ConnectionService;

/// Exports the tables of one request. It only reads, it never changes data.
pub struct Exporter<'run> {
    repository: &'run dyn Repository,
    request: &'run ExportRequest,
}

/// Where `write_table` writes: a text file, or the one sheet of an Excel file.
enum Output<'out> {
    Text(&'out mut (dyn Write + Send)),
    Sheet(&'out mut Worksheet),
}

/// Writes rows into an export file, in one format. It is made with `text` or `excel`,
/// which also write what comes before the rows. Then `push` takes one row at a time,
/// and `finish` writes what comes after the rows.
enum RowWriter<'out> {
    Sql {
        out: &'out mut (dyn Write + Send),
        repository: &'out dyn Repository,
        /// The start of each line: `INSERT INTO t (a, b) VALUES (`.
        insert: String,
    },
    Csv(&'out mut (dyn Write + Send)),
    Json {
        out: &'out mut (dyn Write + Send),
        /// The column names, already as JSON strings.
        keys: Vec<String>,
        /// How many rows are written so far.
        rows: usize,
    },
    Excel {
        sheet: &'out mut Worksheet,
        /// The sheet row of the next table row. Row 0 is the header.
        row: RowNum,
    },
}

impl<'out> RowWriter<'out> {
    /// A writer for a text format. CSV writes its header line here.
    fn text(
        format: ExportFormat,
        out: &'out mut (dyn Write + Send),
        repository: &'out dyn Repository,
        table: &TableRef,
        names: &[String],
    ) -> AppResult<Self> {
        Ok(match format {
            ExportFormat::Sql => Self::Sql {
                insert: Exporter::insert_start(repository, table, names),
                out,
                repository,
            },
            ExportFormat::Csv => {
                let mut header = Vec::with_capacity(names.len());
                for name in names {
                    header.push(Some(name.clone()));
                }
                out.write_all(Exporter::csv_line(&header).as_bytes())?;
                Self::Csv(out)
            }
            ExportFormat::Json => {
                let mut keys = Vec::with_capacity(names.len());
                for name in names {
                    keys.push(Exporter::json_text(name));
                }
                Self::Json { out, keys, rows: 0 }
            }
            ExportFormat::Xlsx => return Err("An Excel file needs a sheet, not a text file".into()),
        })
    }

    /// A writer for the sheet of an Excel file. It writes the header row in bold.
    fn excel(sheet: &'out mut Worksheet, names: &[String]) -> AppResult<Self> {
        let bold = Format::new().set_bold();
        for (column, name) in names.iter().enumerate() {
            sheet.write_string_with_format(0, Self::column(column)?, name, &bold)?;
        }
        Ok(Self::Excel { sheet, row: 1 })
    }

    fn push(&mut self, row: &[Option<String>]) -> AppResult<()> {
        match self {
            Self::Sql {
                out,
                repository,
                insert,
            } => {
                let values = Exporter::insert_values(*repository, row);
                writeln!(out, "{insert}{values});")?;
            }
            Self::Csv(out) => out.write_all(Exporter::csv_line(row).as_bytes())?,
            Self::Json { out, keys, rows } => {
                out.write_all(if *rows == 0 { b"[\n" } else { b",\n" })?;
                out.write_all(Exporter::json_object(keys, row).as_bytes())?;
                *rows += 1;
            }
            // ponytail: every value is a text cell, also numbers, so nothing is rounded
            // and leading zeros stay. Use the column type to write numbers if users ask.
            Self::Excel { sheet, row: number } => {
                for (column, cell) in row.iter().enumerate() {
                    // NULL stays an empty cell.
                    if let Some(value) = cell {
                        sheet.write_string(*number, Self::column(column)?, value)?;
                    }
                }
                *number += 1;
            }
        }
        Ok(())
    }

    fn finish(self) -> AppResult<()> {
        match self {
            Self::Sql { out, .. } => writeln!(out)?,
            Self::Json { out, rows, .. } => {
                out.write_all(if rows == 0 { b"[]\n" } else { b"\n]\n" })?;
            }
            Self::Csv(_) | Self::Excel { .. } => {}
        }
        Ok(())
    }

    /// The sheet column of a table column. Excel has at most 16,384 columns.
    fn column(index: usize) -> AppResult<ColNum> {
        ColNum::try_from(index).map_err(|_| AppError::from("Too many columns for Excel"))
    }
}

impl<'run> Exporter<'run> {
    pub fn new(repository: &'run dyn Repository, request: &'run ExportRequest) -> Self {
        Self {
            repository,
            request,
        }
    }

    /// Writes all tables. CSV, JSON and Excel into a folder give one file per table,
    /// like `<table>.csv`. Everything else goes into the one file at `path`.
    pub async fn run(&self) -> AppResult<()> {
        let request = self.request;
        if request.tables.is_empty() {
            return Err("Choose at least one table to export".into());
        }
        if !request.columns.is_empty() && request.tables.len() != 1 {
            return Err("Columns can only be chosen for one table".into());
        }
        if request.format != ExportFormat::Sql && request.path.is_dir() {
            for table in &request.tables {
                let name = format!("{}.{}", table.name, request.format.extension());
                self.write_file(&request.path.join(name), std::slice::from_ref(table))
                    .await?;
            }
            return Ok(());
        }
        // One JSON array or one sheet holds one table.
        let one_table = matches!(request.format, ExportFormat::Json | ExportFormat::Xlsx);
        if one_table && request.tables.len() > 1 {
            return Err("Export many tables as JSON or Excel into a folder".into());
        }
        self.write_file(&request.path, &request.tables).await
    }

    /// Writes `tables` into a new file at `path`. A file that fails half way is removed,
    /// so a broken export never looks like a complete one.
    async fn write_file(&self, path: &Path, tables: &[TableRef]) -> AppResult<()> {
        let result = self.try_write_file(path, tables).await;
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    async fn try_write_file(&self, path: &Path, tables: &[TableRef]) -> AppResult<()> {
        if self.request.format == ExportFormat::Xlsx {
            // `run` makes sure this is one table.
            let mut sheet = Worksheet::new();
            for table in tables {
                self.write_table(Output::Sheet(&mut sheet), table).await?;
            }
            return Self::save_sheet(sheet, path);
        }
        let mut out = BufWriter::new(File::create(path)?);
        for table in tables {
            self.write_table(Output::Text(&mut out), table).await?;
        }
        out.flush()?;
        Ok(())
    }

    async fn write_table(&self, out: Output<'_>, table: &TableRef) -> AppResult<()> {
        let repository = self.repository;
        let all = repository.columns(table).await?;
        let chosen = self.chosen(&all)?;
        let mut names = Vec::with_capacity(chosen.len());
        let mut select = Vec::with_capacity(chosen.len());
        for column in &chosen {
            names.push(column.name.clone());
            select.push(repository.select_column(column));
        }
        let mut writer = match out {
            Output::Sheet(sheet) => RowWriter::excel(sheet, &names)?,
            Output::Text(out) => {
                let sql = self.request.format == ExportFormat::Sql;
                if sql && self.request.structure {
                    write!(out, "{};\n\n", self.create_statement(table, &all).await?)?;
                }
                if sql && !self.request.data {
                    return Ok(());
                }
                RowWriter::text(self.request.format, out, repository, table, &names)?
            }
        };
        let name = repository.table_name(table);
        // Sort by the primary key, so no row is read twice or missed between pages.
        let mut keys = Vec::new();
        for column in &all {
            if column.primary_key {
                keys.push(repository.quote(&column.name));
            }
        }
        let ordering = if keys.is_empty() {
            String::new()
        } else {
            format!(" ORDER BY {}", keys.join(", "))
        };
        // `query` keeps at most MAX_QUERY_ROWS rows, so one page is exactly that many.
        // ponytail: LIMIT/OFFSET pages get slower deep into a very large table, and a
        // table without a primary key has no fixed order between pages. Page by key
        // ranges, or stream the rows, if that hurts.
        let mut offset: u64 = 0;
        loop {
            let select = format!(
                "SELECT {} FROM {name}{}",
                select.join(", "),
                repository.page_sql(&ordering, MAX_QUERY_ROWS, offset)
            );
            let rows = repository
                .query(&select)
                .await?
                .pop()
                .unwrap_or_default()
                .rows;
            for row in &rows {
                writer.push(row)?;
            }
            if rows.len() < MAX_QUERY_ROWS {
                break;
            }
            offset += MAX_QUERY_ROWS as u64;
        }
        writer.finish()
    }

    /// Writes rows the app already has, in the format of the request. SQL becomes
    /// `INSERT` statements into `request.table`. The rows are small enough to sit
    /// in memory, so the whole file is built first and written in one go.
    pub fn write_rows(repository: &dyn Repository, request: &RowsExport) -> AppResult<()> {
        if request.format == ExportFormat::Xlsx {
            let mut sheet = Worksheet::new();
            let mut writer = RowWriter::excel(&mut sheet, &request.columns)?;
            for row in &request.rows {
                writer.push(row)?;
            }
            writer.finish()?;
            return Self::save_sheet(sheet, &request.path);
        }
        let mut text = Vec::new();
        let mut writer = RowWriter::text(
            request.format,
            &mut text,
            repository,
            &request.table,
            &request.columns,
        )?;
        for row in &request.rows {
            writer.push(row)?;
        }
        writer.finish()?;
        std::fs::write(&request.path, text)?;
        Ok(())
    }

    /// Saves one sheet as an Excel file at `path`.
    fn save_sheet(sheet: Worksheet, path: &Path) -> AppResult<()> {
        let mut book = Workbook::new();
        book.push_worksheet(sheet);
        book.save(path)?;
        Ok(())
    }

    /// One `INSERT` statement per row into `table`, each on its own line.
    /// Names and values are quoted the way the database of `repository` wants.
    pub fn insert_sql(
        repository: &dyn Repository,
        table: &TableRef,
        columns: &[String],
        rows: &[Vec<Option<String>>],
    ) -> String {
        let insert = Self::insert_start(repository, table, columns);
        let mut text = String::new();
        for row in rows {
            text.push_str(&insert);
            text.push_str(&Self::insert_values(repository, row));
            text.push_str(");\n");
        }
        text
    }

    /// The start of an `INSERT` statement up to the values: `INSERT INTO t (a, b) VALUES (`.
    fn insert_start(repository: &dyn Repository, table: &TableRef, columns: &[String]) -> String {
        let mut names = Vec::with_capacity(columns.len());
        for name in columns {
            names.push(repository.quote(name));
        }
        format!(
            "INSERT INTO {} ({}) VALUES (",
            repository.table_name(table),
            names.join(", ")
        )
    }

    /// One row as a JSON object, indented for an array, like the "Copy Rows As JSON"
    /// of the frontend. `keys` are the column names as JSON strings. NULL becomes null.
    /// Built by hand, so the keys keep the column order.
    fn json_object(keys: &[String], row: &[Option<String>]) -> String {
        let mut fields = Vec::with_capacity(keys.len());
        for (key, cell) in keys.iter().zip(row) {
            let value = match cell {
                Some(text) => Self::json_text(text),
                None => "null".to_owned(),
            };
            fields.push(format!("    {key}: {value}"));
        }
        format!("  {{\n{}\n  }}", fields.join(",\n"))
    }

    /// A text as a JSON string, with quotes and escapes.
    fn json_text(text: &str) -> String {
        serde_json::Value::from(text).to_string()
    }

    /// The columns to export, in the order the user chose. Stops names that are not in the table.
    fn chosen(&self, all: &[ColumnInfo]) -> AppResult<Vec<ColumnInfo>> {
        if self.request.columns.is_empty() {
            return Ok(all.to_vec());
        }
        let mut chosen = Vec::with_capacity(self.request.columns.len());
        for name in &self.request.columns {
            chosen.push(ConnectionService::column(all, name)?.clone());
        }
        Ok(chosen)
    }

    /// The database's own `CREATE` statement. Without one, it is built from the columns.
    // ponytail: the built statement (PostgreSQL) has the columns and the primary key,
    // but no indexes, foreign keys or checks. The Backup button (pg_dump) makes a full copy.
    async fn create_statement(
        &self,
        table: &TableRef,
        columns: &[ColumnInfo],
    ) -> AppResult<String> {
        Self::create_table_sql(self.repository, table, columns).await
    }

    /// See `create_statement`. Schema compare uses it for a table that is missing.
    pub(super) async fn create_table_sql(
        repository: &dyn Repository,
        table: &TableRef,
        columns: &[ColumnInfo],
    ) -> AppResult<String> {
        if let Some(sql) = repository.create_sql(table).await? {
            return Ok(sql);
        }
        let mut parts = Vec::with_capacity(columns.len() + 1);
        let mut keys = Vec::new();
        for column in columns {
            let mut part = format!("{} {}", repository.quote(&column.name), column.data_type);
            if column.extra == "identity" {
                part.push(' ');
                part.push_str(repository.auto_increment());
            }
            if !column.nullable {
                part.push_str(" NOT NULL");
            }
            if let Some(default) = &column.default {
                part.push_str(" DEFAULT ");
                part.push_str(default);
            }
            if column.primary_key {
                keys.push(repository.quote(&column.name));
            }
            parts.push(part);
        }
        if !keys.is_empty() {
            parts.push(format!("PRIMARY KEY ({})", keys.join(", ")));
        }
        Ok(format!(
            "CREATE TABLE {} (\n  {}\n)",
            repository.table_name(table),
            parts.join(",\n  ")
        ))
    }

    /// The values of one row for an `INSERT`. NULL stays NULL. Every other value is
    /// a quoted text. All three databases turn such a text into the column type.
    // ponytail: binary values that are not UTF-8 arrive as `0x…` text and are written
    // as that text, not as bytes. Read them as bytes if binary columns must round trip.
    fn insert_values(repository: &dyn Repository, row: &[Option<String>]) -> String {
        let mut values = Vec::with_capacity(row.len());
        for cell in row {
            values.push(match cell {
                Some(value) => repository.literal(value),
                None => "NULL".to_owned(),
            });
        }
        values.join(", ")
    }

    /// One CSV line as RFC 4180 writes it, with CRLF at the end.
    /// A field with a comma, a quote or a line break is quoted. NULL is an empty field.
    pub fn csv_line(cells: &[Option<String>]) -> String {
        let mut line = String::new();
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            let text = cell.as_deref().unwrap_or_default();
            if text.contains([',', '"', '\r', '\n']) {
                line.push('"');
                line.push_str(&text.replace('"', "\"\""));
                line.push('"');
            } else {
                line.push_str(text);
            }
        }
        line.push_str("\r\n");
        line
    }
}

#[cfg(test)]
mod tests {
    use super::Exporter;

    #[test]
    fn csv_quotes_only_what_needs_it() {
        let cells = [
            Some("plain".to_owned()),
            None,
            Some("a,b".to_owned()),
            Some("say \"hi\"".to_owned()),
            Some("two\nlines".to_owned()),
        ];
        assert_eq!(
            Exporter::csv_line(&cells),
            "plain,,\"a,b\",\"say \"\"hi\"\"\",\"two\nlines\"\r\n"
        );
    }

    #[test]
    fn json_keeps_the_column_order_and_escapes_text() {
        let keys = [Exporter::json_text("z"), Exporter::json_text("a")];
        let row = [Some("say \"hi\"\n".to_owned()), None];
        assert_eq!(
            Exporter::json_object(&keys, &row),
            "  {\n    \"z\": \"say \\\"hi\\\"\\n\",\n    \"a\": null\n  }"
        );
    }
}
