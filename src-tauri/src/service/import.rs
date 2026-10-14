//! Reads files into the database: CSV rows into one table, or a SQL file into the database.

use std::path::Path;

use crate::constants::CSV_PREVIEW_ROWS;
use crate::error::{AppError, AppResult};
use crate::model::{CsvImport, CsvPreview, Driver, Values};
use crate::repository::Statement;

use super::ConnectionService;

// ponytail: both imports read the whole file into memory first. That is fine up to a few
// hundred MB. Stream the file if bigger imports are needed.
impl ConnectionService {
    /// The first rows of a CSV file and how many rows it has. It does not need a connection.
    pub fn preview_csv(path: &Path) -> AppResult<CsvPreview> {
        let records = CsvParser::parse(&std::fs::read_to_string(path)?)?;
        let mut rows = Vec::with_capacity(CSV_PREVIEW_ROWS);
        for record in records.iter().take(CSV_PREVIEW_ROWS) {
            rows.push(record.fields.clone());
        }
        Ok(CsvPreview {
            rows,
            count: records.len(),
        })
    }

    /// Inserts the rows of a CSV file into a table, in one transaction. All or nothing.
    /// An error names the line of the file. Gives the number of inserted rows.
    // ponytail: one INSERT per row, like `save_changes`. Fine for tens of thousands of rows.
    // Send many rows per INSERT if large files are too slow.
    pub async fn import_csv(&self, id: &str, request: &CsvImport) -> AppResult<usize> {
        if request.columns.is_empty() {
            return Err("Choose at least one column to import".into());
        }
        let repository = self.repository(id)?;
        let records = CsvParser::parse(&std::fs::read_to_string(&request.path)?)?;
        let columns = repository.columns(&request.table).await?;
        let mut statements = Vec::with_capacity(records.len());
        for record in records.into_iter().skip(usize::from(request.header)) {
            let label = format!("Line {}", record.line);
            let mut values = Values::new();
            for column in &request.columns {
                let Some(field) = record.fields.get(column.field) else {
                    return Err(AppError::Message(format!(
                        "{label} has only {} fields",
                        record.fields.len()
                    )));
                };
                let empty = field.is_empty() && request.empty_is_null;
                values.insert(column.name.clone(), (!empty).then(|| field.clone()));
            }
            let built = Self::insert_sql(repository.as_ref(), &request.table, &columns, values);
            statements.push(Self::statement(label, built)?);
        }
        if statements.is_empty() {
            return Err("The file has no rows to import".into());
        }
        let count = statements.len();
        repository.execute_all(statements).await?;
        Ok(count)
    }

    /// Runs all statements of a SQL file on the connection of the tab, in one transaction.
    /// Gives the number of statements.
    pub async fn import_sql(&self, id: &str, path: &Path) -> AppResult<usize> {
        let connection = self.connection(id)?;
        let sql = std::fs::read_to_string(path)?;
        let statements = SqlSplitter::new(&sql, connection.config.driver == Driver::Mysql).split();
        if statements.is_empty() {
            return Err("The file has no SQL statements".into());
        }
        let count = statements.len();
        connection.repository.run_script(statements).await?;
        Ok(count)
    }
}

/// One row of a CSV file.
pub struct CsvRecord {
    /// The line of the file where the row starts. The first line is 1.
    pub line: usize,
    pub fields: Vec<String>,
}

/// Reads CSV as RFC 4180 writes it: comma between fields, double quotes around a field
/// with a comma, quote or line break, and `""` for a quote inside quotes.
/// Line ends may be CRLF or LF. Empty lines are skipped. A UTF-8 BOM at the start is skipped.
pub struct CsvParser;

impl CsvParser {
    pub fn parse(text: &str) -> AppResult<Vec<CsvRecord>> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut records = Vec::new();
        let mut fields = Vec::new();
        let mut field = String::new();
        let mut quoted = false;
        // True when the current row has anything in it. An empty line has nothing.
        let mut started = false;
        let mut line = 1;
        let mut start = 1;
        let mut chars = text.chars().peekable();
        while let Some(char) = chars.next() {
            if quoted {
                match char {
                    '"' if chars.peek() == Some(&'"') => {
                        chars.next();
                        field.push('"');
                    }
                    '"' => quoted = false,
                    _ => {
                        if char == '\n' {
                            line += 1;
                        }
                        field.push(char);
                    }
                }
                continue;
            }
            match char {
                // A quote opens a quoted field only at its start. Elsewhere it is plain text.
                '"' if field.is_empty() => {
                    quoted = true;
                    started = true;
                }
                ',' => {
                    fields.push(std::mem::take(&mut field));
                    started = true;
                }
                '\r' if chars.peek() == Some(&'\n') => {}
                '\r' | '\n' => {
                    if started {
                        fields.push(std::mem::take(&mut field));
                        records.push(CsvRecord {
                            line: start,
                            fields: std::mem::take(&mut fields),
                        });
                    }
                    started = false;
                    line += 1;
                    start = line;
                }
                _ => {
                    field.push(char);
                    started = true;
                }
            }
        }
        if quoted {
            return Err(AppError::Message(format!(
                "Line {start}: a quoted field is not closed"
            )));
        }
        if started {
            fields.push(field);
            records.push(CsvRecord {
                line: start,
                fields,
            });
        }
        Ok(records)
    }
}

/// Splits a SQL file into statements at the `;` between them.
///
/// A `;` inside a text, a quoted name or a comment does not split. `MySQL` files may
/// change the delimiter with a `DELIMITER //` line, like the mysql client reads them.
/// `PostgreSQL` files may hold `$$ … $$` texts, like function bodies.
/// `BEGIN` and `COMMIT` lines are left out, because the whole file runs in one transaction.
// ponytail: PostgreSQL `E'…'` texts with a backslash before a quote, and
// `COPY … FROM stdin` blocks of pg_dump, are not understood. Add them if such files must import.
pub struct SqlSplitter<'sql> {
    sql: &'sql str,
    bytes: &'sql [u8],
    /// `MySQL` rules: a backslash escapes inside texts, `#` starts a comment, `DELIMITER` works.
    mysql: bool,
    position: usize,
    line: usize,
}

impl<'sql> SqlSplitter<'sql> {
    pub fn new(sql: &'sql str, mysql: bool) -> Self {
        Self {
            sql,
            bytes: sql.as_bytes(),
            mysql,
            position: 0,
            line: 1,
        }
    }

    /// The statements without their delimiter. Each is labeled with the line it starts on.
    /// The text of a statement keeps its comments, because `MySQL` runs `/*! … */` comments.
    pub fn split(mut self) -> Vec<Statement> {
        let mut statements = Vec::new();
        let mut delimiter = ";".to_owned();
        let mut begin = 0;
        // The line where the code of the current statement starts. None while there is no code.
        let mut code_line = None;
        while self.position < self.bytes.len() {
            let bytes = self.bytes;
            let rest = &bytes[self.position..];
            if code_line.is_none() && self.mysql && self.at_delimiter_command() {
                let end = self.line_end();
                let new = self.sql[self.position + "DELIMITER".len()..end].trim();
                if !new.is_empty() {
                    new.clone_into(&mut delimiter);
                }
                self.position = end;
                begin = end;
                continue;
            }
            if rest.starts_with(delimiter.as_bytes()) {
                if let Some(line) = code_line.take() {
                    Self::keep(&mut statements, &self.sql[begin..self.position], line);
                }
                self.position += delimiter.len();
                begin = self.position;
                continue;
            }
            let byte = rest[0];
            let next = rest.get(1).copied();
            let is_code = match (byte, next) {
                (b'\'' | b'"' | b'`', _) => {
                    self.skip_quoted(byte);
                    true
                }
                (b'-', Some(b'-')) => {
                    self.position = self.line_end();
                    false
                }
                (b'#', _) if self.mysql => {
                    self.position = self.line_end();
                    false
                }
                (b'/', Some(b'*')) => {
                    // `MySQL` runs the SQL inside `/*! … */`.
                    let runs = self.mysql && rest.get(2) == Some(&b'!');
                    self.skip_past(2, "*/");
                    runs
                }
                (b'$', _) if !self.mysql && self.dollar_tag().is_some() => {
                    let tag = self.dollar_tag().unwrap_or_default();
                    self.skip_past(tag.len(), tag);
                    true
                }
                _ => {
                    if byte == b'\n' {
                        self.line += 1;
                    }
                    self.position += 1;
                    !byte.is_ascii_whitespace()
                }
            };
            if is_code && code_line.is_none() {
                code_line = Some(self.line);
            }
        }
        if let Some(line) = code_line {
            Self::keep(&mut statements, &self.sql[begin..], line);
        }
        statements
    }

    /// Adds a statement, unless it only starts or ends a transaction.
    fn keep(statements: &mut Vec<Statement>, sql: &str, line: usize) {
        let words = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        let control = [
            "BEGIN",
            "BEGIN TRANSACTION",
            "START TRANSACTION",
            "COMMIT",
            "COMMIT TRANSACTION",
            "END",
            "END TRANSACTION",
        ];
        if control.contains(&words.to_ascii_uppercase().as_str()) {
            return;
        }
        statements.push(Statement {
            label: format!("Statement at line {line}"),
            sql: sql.trim().to_owned(),
            params: Vec::new(),
        });
    }

    /// True at a `DELIMITER` line of the mysql client.
    fn at_delimiter_command(&self) -> bool {
        let at_line_start = self.position == 0 || self.bytes[self.position - 1] == b'\n';
        let rest = &self.bytes[self.position..];
        let word = b"DELIMITER";
        at_line_start
            && rest.len() > word.len()
            && rest[..word.len()].eq_ignore_ascii_case(word)
            && rest[word.len()].is_ascii_whitespace()
    }

    /// Where the current line ends: at its line break, or at the end of the file.
    fn line_end(&self) -> usize {
        let rest = &self.bytes[self.position..];
        let found = rest.iter().position(|byte| *byte == b'\n');
        found.map_or(self.bytes.len(), |offset| self.position + offset)
    }

    /// Moves past a quoted text or name that starts at the current position.
    /// A doubled quote stays inside. In `MySQL` texts a backslash escapes the next character.
    fn skip_quoted(&mut self, quote: u8) {
        let escapes = self.mysql && quote != b'`';
        self.position += 1;
        while self.position < self.bytes.len() {
            let byte = self.bytes[self.position];
            self.position += 1;
            if byte == b'\n' {
                self.line += 1;
            } else if escapes && byte == b'\\' {
                if self.bytes.get(self.position) == Some(&b'\n') {
                    self.line += 1;
                }
                self.position += 1;
            } else if byte == quote {
                return;
            }
        }
    }

    /// Moves past `open` bytes, then past the next `close`, or to the end of the file.
    fn skip_past(&mut self, open: usize, close: &str) {
        let start = (self.position + open).min(self.bytes.len());
        let rest = &self.sql[start..];
        let end = rest
            .find(close)
            .map_or(self.bytes.len(), |offset| start + offset + close.len());
        self.line += self.sql[self.position..end].matches('\n').count();
        self.position = end;
    }

    /// The `$tag$` of a `PostgreSQL` dollar quoted text that starts here, like `$$` or `$body$`.
    fn dollar_tag(&self) -> Option<&'sql str> {
        let before = self.position.checked_sub(1).map(|index| self.bytes[index]);
        if before.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            return None;
        }
        let rest = &self.bytes[self.position + 1..];
        let length = rest
            .iter()
            .position(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'))?;
        if rest[length] != b'$' || rest.first().is_some_and(u8::is_ascii_digit) {
            return None;
        }
        Some(&self.sql[self.position..self.position + length + 2])
    }
}

#[cfg(test)]
mod tests {
    use super::{CsvParser, SqlSplitter};

    fn fields(text: &str) -> Vec<(usize, Vec<String>)> {
        let mut rows = Vec::new();
        for record in CsvParser::parse(text).expect("csv") {
            rows.push((record.line, record.fields));
        }
        rows
    }

    fn row(line: usize, cells: &[&str]) -> (usize, Vec<String>) {
        (line, cells.iter().map(|cell| (*cell).to_owned()).collect())
    }

    #[test]
    fn csv_reads_quotes_line_breaks_and_empty_fields() {
        let text = "\u{feff}id,name,note\r\n1,\"a,b\",\"say \"\"hi\"\"\"\r\n\r\n2,,\"two\nlines\"\n3,5\" tall,x";
        assert_eq!(
            fields(text),
            vec![
                row(1, &["id", "name", "note"]),
                row(2, &["1", "a,b", "say \"hi\""]),
                row(4, &["2", "", "two\nlines"]),
                row(6, &["3", "5\" tall", "x"]),
            ]
        );
        // A quoted empty field is still a field.
        assert_eq!(fields("\"\"\n"), vec![row(1, &[""])]);
    }

    #[test]
    fn csv_names_the_line_of_an_open_quote() {
        let error = CsvParser::parse("a\nb\n\"c\nd").err().expect("error");
        assert_eq!(error.to_string(), "Line 3: a quoted field is not closed");
    }

    fn split(sql: &str, mysql: bool) -> Vec<(String, String)> {
        let mut parts = Vec::new();
        for statement in SqlSplitter::new(sql, mysql).split() {
            parts.push((statement.label, statement.sql));
        }
        parts
    }

    fn part(line: usize, sql: &str) -> (String, String) {
        (format!("Statement at line {line}"), sql.to_owned())
    }

    #[test]
    fn sql_splits_only_outside_texts_and_comments() {
        let sql = "-- a; comment\nCREATE TABLE \"a;b\" (x TEXT);\n\
            INSERT INTO t VALUES ('it''s; fine', \"q\");\n\
            /* block; */ SELECT 1;\n-- only a comment;\n";
        assert_eq!(
            split(sql, false),
            vec![
                part(2, "-- a; comment\nCREATE TABLE \"a;b\" (x TEXT)"),
                part(3, "INSERT INTO t VALUES ('it''s; fine', \"q\")"),
                part(4, "/* block; */ SELECT 1"),
            ]
        );
    }

    #[test]
    fn sql_drops_transaction_lines() {
        let sql = "BEGIN TRANSACTION;\nCREATE TABLE t (x);\nCOMMIT;";
        assert_eq!(split(sql, false), vec![part(2, "CREATE TABLE t (x)")]);
    }

    #[test]
    fn sql_knows_mysql_escapes_and_delimiters() {
        let sql = "INSERT INTO t VALUES ('a\\';b', `c;d`); # note;\n\
            /*!40101 SET NAMES utf8 */;\n\
            DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END//\nDELIMITER ;\nSELECT 2;";
        assert_eq!(
            split(sql, true),
            vec![
                part(1, "INSERT INTO t VALUES ('a\\';b', `c;d`)"),
                part(2, "# note;\n/*!40101 SET NAMES utf8 */"),
                part(4, "CREATE PROCEDURE p() BEGIN SELECT 1; END"),
                part(6, "SELECT 2"),
            ]
        );
    }

    #[test]
    fn sql_knows_postgres_dollar_quotes() {
        let sql = "CREATE FUNCTION f() RETURNS int AS $body$ SELECT 1; $body$ LANGUAGE sql;\n\
            SELECT $$a;b$$, 'c\\';";
        assert_eq!(
            split(sql, false),
            vec![
                part(
                    1,
                    "CREATE FUNCTION f() RETURNS int AS $body$ SELECT 1; $body$ LANGUAGE sql"
                ),
                part(2, "SELECT $$a;b$$, 'c\\'"),
            ]
        );
    }
}
