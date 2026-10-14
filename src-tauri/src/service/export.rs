//! Writes tables to a file: as SQL (`CREATE` and `INSERT` statements) or as CSV.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

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

impl<'run> Exporter<'run> {
    pub fn new(repository: &'run dyn Repository, request: &'run ExportRequest) -> Self {
        Self {
            repository,
            request,
        }
    }

    /// Writes all tables. CSV into a folder gives one `<table>.csv` file per table.
    /// Everything else goes into the one file at `path`.
    pub async fn run(&self) -> AppResult<()> {
        let request = self.request;
        if request.tables.is_empty() {
            return Err("Choose at least one table to export".into());
        }
        if !request.columns.is_empty() && request.tables.len() != 1 {
            return Err("Columns can only be chosen for one table".into());
        }
        if request.format == ExportFormat::Csv && request.path.is_dir() {
            for table in &request.tables {
                let path = request.path.join(format!("{}.csv", table.name));
                self.write_file(&path, std::slice::from_ref(table)).await?;
            }
            return Ok(());
        }
        self.write_file(&request.path, &request.tables).await
    }

    /// Writes `tables` into a new file at `path`. A file that fails half way is removed,
    /// so a broken export never looks like a complete one.
    async fn write_file(&self, path: &Path, tables: &[TableRef]) -> AppResult<()> {
        let mut out = BufWriter::new(File::create(path)?);
        let mut result = Ok(());
        for table in tables {
            result = self.write_table(&mut out, table).await;
            if result.is_err() {
                break;
            }
        }
        if result.is_ok() {
            result = out.flush().map_err(AppError::from);
        }
        if result.is_err() {
            drop(out);
            let _ = std::fs::remove_file(path);
        }
        result
    }

    async fn write_table(&self, out: &mut impl Write, table: &TableRef) -> AppResult<()> {
        let repository = self.repository;
        let all = repository.columns(table).await?;
        let chosen = self.chosen(&all)?;
        let sql = self.request.format == ExportFormat::Sql;
        if sql && self.request.structure {
            write!(out, "{};\n\n", self.create_statement(table, &all).await?)?;
        }
        if sql && !self.request.data {
            return Ok(());
        }
        let mut names = Vec::with_capacity(chosen.len());
        for column in &chosen {
            names.push(repository.quote(&column.name));
        }
        let name = repository.table_name(table);
        let insert = format!("INSERT INTO {name} ({}) VALUES (", names.join(", "));
        if !sql {
            let mut header = Vec::with_capacity(chosen.len());
            for column in &chosen {
                header.push(Some(column.name.clone()));
            }
            out.write_all(Self::csv_line(&header).as_bytes())?;
        }
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
        let mut offset = 0;
        loop {
            let select = format!(
                "SELECT {} FROM {name}{ordering} LIMIT {MAX_QUERY_ROWS} OFFSET {offset}",
                names.join(", ")
            );
            let rows = repository
                .query(&select)
                .await?
                .pop()
                .unwrap_or_default()
                .rows;
            for row in &rows {
                if sql {
                    let line = Self::insert_values(repository, row);
                    writeln!(out, "{insert}{line});")?;
                } else {
                    out.write_all(Self::csv_line(row).as_bytes())?;
                }
            }
            if rows.len() < MAX_QUERY_ROWS {
                break;
            }
            offset += MAX_QUERY_ROWS;
        }
        if sql {
            writeln!(out)?;
        }
        Ok(())
    }

    /// Writes rows the app already has, as CSV with a header line or as `INSERT`
    /// statements into `request.table`. The rows are small enough to sit in memory,
    /// so the whole file is built first and written in one go.
    pub fn write_rows(repository: &dyn Repository, request: &RowsExport) -> AppResult<()> {
        let mut text = String::new();
        if request.format == ExportFormat::Csv {
            let mut header = Vec::with_capacity(request.columns.len());
            for name in &request.columns {
                header.push(Some(name.clone()));
            }
            text.push_str(&Self::csv_line(&header));
            for row in &request.rows {
                text.push_str(&Self::csv_line(row));
            }
        } else {
            let mut names = Vec::with_capacity(request.columns.len());
            for name in &request.columns {
                names.push(repository.quote(name));
            }
            let insert = format!(
                "INSERT INTO {} ({}) VALUES (",
                repository.table_name(&request.table),
                names.join(", ")
            );
            for row in &request.rows {
                text.push_str(&insert);
                text.push_str(&Self::insert_values(repository, row));
                text.push_str(");\n");
            }
        }
        std::fs::write(&request.path, text)?;
        Ok(())
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
    // but no indexes, foreign keys or checks. A view comes out as a table. The Backup
    // button (pg_dump) makes a full copy.
    async fn create_statement(
        &self,
        table: &TableRef,
        columns: &[ColumnInfo],
    ) -> AppResult<String> {
        let repository = self.repository;
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
}
