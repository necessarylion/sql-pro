//! The logic of "Ask Claude": what we tell Claude about the database, how we read
//! its reply, and whether its SQL only reads.

use std::fmt::Write as _;

use crate::constants::{
    AI_PROVIDERS, CLAUDE_EFFORTS, CLAUDE_MODEL_ALIASES, CLAUDE_SCHEMA_MAX_CHARS, EXPLAIN_KEYWORDS,
    MODEL_ID_MAX, READ_KEYWORDS, WRITE_KEYWORDS,
};
use crate::error::{AppError, AppResult};
use crate::model::{AiOptions, ColumnInfo, Driver, TableRef};

/// Checks the model and effort against fixed lists. They go on the command line,
/// so free text must never get through.
pub(super) fn check_options(options: &AiOptions) -> AppResult<()> {
    if !AI_PROVIDERS.contains(&options.provider.as_str()) {
        return Err(AppError::Message(format!(
            "Unknown AI provider: {}",
            options.provider
        )));
    }
    if let Some(model) = &options.model
        && !is_model_id(model)
    {
        return Err(AppError::Message(format!(
            "Not a valid AI model id: {model}"
        )));
    }
    if let Some(effort) = &options.effort
        && !CLAUDE_EFFORTS.contains(&effort.as_str())
    {
        return Err(AppError::Message(format!("Unknown AI effort: {effort}")));
    }
    Ok(())
}

/// True for a known alias, or an id like `claude-opus-5-5` or `claude-opus-5-5[1m]`:
/// lower case letters, digits, `.`, `-`, `[` and `]`, at most `MODEL_ID_MAX` long.
/// It never starts with `-`, so it cannot be read as a flag. It goes to `claude` as
/// its own argument, never through a shell.
pub(super) fn is_model_id(model: &str) -> bool {
    if CLAUDE_MODEL_ALIASES.contains(&model) {
        return true;
    }
    let mut chars = model.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    model.len() <= MODEL_ID_MAX
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|char| {
            char.is_ascii_lowercase()
                || char.is_ascii_digit()
                || matches!(char, '.' | '-' | '[' | ']')
        })
}

/// The name of the database kind, as Claude knows it.
pub(super) fn dialect(driver: Driver) -> &'static str {
    match driver {
        Driver::Mysql => "MySQL / MariaDB",
        Driver::Postgres => "PostgreSQL",
        Driver::Sqlite => "SQLite",
        Driver::Mssql => "Microsoft SQL Server (T-SQL)",
    }
}

/// The first message of a conversation: the database, its tables, then the question.
/// The table list is cut at `CLAUDE_SCHEMA_MAX_CHARS`.
pub(super) fn first_prompt(
    driver: Driver,
    database: &str,
    tables: &[(TableRef, Vec<ColumnInfo>)],
    question: &str,
) -> String {
    let mut schema = String::new();
    for (table, columns) in tables {
        let mut line = String::new();
        if let Some(schema) = &table.schema {
            let _ = write!(line, "{schema}.");
        }
        let _ = write!(line, "{}", table.name);
        if !table.kind.is_empty() && table.kind != "table" {
            let _ = write!(line, " [{}]", table.kind);
        }
        let mut parts = Vec::with_capacity(columns.len());
        for column in columns {
            let key = if column.primary_key { " PK" } else { "" };
            parts.push(format!("{} {}{key}", column.name, column.data_type));
        }
        let _ = writeln!(line, "({})", parts.join(", "));
        if schema.len() + line.len() > CLAUDE_SCHEMA_MAX_CHARS {
            schema.push_str("(more tables are not listed)\n");
            break;
        }
        schema.push_str(&line);
    }
    format!(
        "Database kind: {}\nCurrent database: {database}\nTables (name(column type, ...)):\n{schema}\nQuestion: {question}",
        dialect(driver)
    )
}

/// Reads Claude's reply text as `{"sql": ..., "message": ...}`.
/// The JSON may sit in a Markdown code block or between other text. When it is not JSON,
/// the whole text is the message and there is no SQL.
pub(super) fn parse_reply(text: &str) -> (Option<String>, String) {
    let start = text.find('{');
    let end = text.rfind('}');
    if let (Some(start), Some(end)) = (start, end)
        && start < end
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text[start..=end])
        && let Some(message) = value.get("message").and_then(serde_json::Value::as_str)
    {
        return (sql_of(&value), message.to_owned());
    }
    (None, text.trim().to_owned())
}

/// The `sql` field of a reply. Blank SQL counts as none.
pub(super) fn sql_of(value: &serde_json::Value) -> Option<String> {
    let sql = value.get("sql")?.as_str()?.trim();
    (!sql.is_empty()).then(|| sql.to_owned())
}

/// True when `sql` surely only reads. Anything we are not sure about counts as a write,
/// so the user is asked first. A wrong "write" only costs one click. A wrong "read" can lose data.
///
/// A read is one statement (one `;` at the end is fine) that starts with a word of
/// `READ_KEYWORDS` and holds no word of `WRITE_KEYWORDS` outside strings and comments.
///
/// ponytail: no real SQL parser. Text we cannot read for sure counts as a write:
/// backslashes, `#`, `$`, `--` with no space after it, `/*!` and nested comments.
/// A SELECT that calls a function with side effects (like `nextval`) still counts as a read.
pub fn is_read_only(sql: &str) -> bool {
    let Some(code) = code_only(sql) else {
        return false;
    };
    let code = code.trim_end();
    let code = code.strip_suffix(';').unwrap_or(code);
    if code.contains(';') {
        return false;
    }
    let mut words = code
        .split(|char: char| !(char.is_ascii_alphanumeric() || char == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_uppercase);
    let Some(first) = words.next() else {
        return false;
    };
    READ_KEYWORDS.contains(&first.as_str())
        && !words.any(|word| WRITE_KEYWORDS.contains(&word.as_str()))
}

/// `sql` without its `;` at the end, when it is one statement that Explain may take.
/// `None` for anything else. A second statement would run for real behind the
/// `EXPLAIN`, and `ANALYZE` in front would run the first one.
///
/// ponytail: same text check as `is_read_only`. Text it cannot read for sure
/// (`$`, `#`, backslashes) is refused.
pub(super) fn explainable(sql: &str) -> Option<&str> {
    let code = code_only(sql)?;
    let code = code.trim_end();
    let code = code.strip_suffix(';').unwrap_or(code);
    let first = words(code).next()?;
    if code.contains(';') || !EXPLAIN_KEYWORDS.contains(&first.as_str()) {
        return None;
    }
    let sql = sql.trim_end();
    Some(sql.strip_suffix(';').unwrap_or(sql))
}

/// How `sql` from the Query view changes the transaction state of the connection:
/// `Some(true)` it opens one, `Some(false)` it ends it, `None` no change.
/// Only `MySQL` needs this: it cannot tell if `BEGIN` ran. The last statement wins.
///
/// ponytail: only the first words of each statement are read. When we cannot read
/// the text for sure, a word like `BEGIN` anywhere counts as open. A wrong "open"
/// only blocks AI reads until the user runs `COMMIT` or `ROLLBACK`.
pub(super) fn transaction_change(sql: &str) -> Option<bool> {
    let Some(code) = code_only(sql) else {
        let upper = sql.to_ascii_uppercase();
        let opens = ["BEGIN", "START", "LOCK", "AUTOCOMMIT", "XA"];
        return words(&upper)
            .any(|word| opens.contains(&word.as_str()))
            .then_some(true);
    };
    let mut change = None;
    for statement in code.split(';') {
        let all: Vec<String> = words(statement).collect();
        let first = all.first().map_or("", String::as_str);
        let second = all.get(1).map_or("", String::as_str);
        let autocommit = all.iter().any(|word| word == "AUTOCOMMIT");
        change = match (first, second) {
            ("BEGIN" | "LOCK", _) | ("START", "TRANSACTION") | ("XA", "START" | "BEGIN") => {
                Some(true)
            }
            ("SET", _) if autocommit => {
                let off = all.iter().any(|word| word == "0" || word == "OFF");
                Some(off)
            }
            ("ROLLBACK", "TO") => change,
            ("COMMIT" | "ROLLBACK" | "UNLOCK", _) => Some(false),
            _ => change,
        };
    }
    change
}

/// The words of `text`, in upper case.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|char: char| !(char.is_ascii_alphanumeric() || char == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_uppercase)
}

/// `sql` with strings, quoted names and comments turned into spaces.
/// `None` when the text has something we cannot read for sure.
fn code_only(sql: &str) -> Option<String> {
    // Backslash escapes depend on the database and its settings. `$` starts a
    // PostgreSQL string. `#` starts a MySQL comment. Not worth the risk.
    if sql.contains(['\\', '$', '#']) {
        return None;
    }
    let chars: Vec<char> = sql.chars().collect();
    let mut code = String::with_capacity(sql.len());
    let mut index = 0;
    while index < chars.len() {
        let char = chars[index];
        let next = chars.get(index + 1).copied();
        let end = match (char, next) {
            ('\'' | '"' | '`', _) => close(&chars, index + 1, char)?,
            ('[', _) => close(&chars, index + 1, ']')?,
            ('-', Some('-')) => {
                // MySQL reads `--` as a comment only when a space follows. `1--1` is math there.
                if !chars
                    .get(index + 2)
                    .is_none_or(|after| after.is_whitespace())
                {
                    return None;
                }
                chars[index..]
                    .iter()
                    .position(|&c| c == '\n')
                    .map_or(chars.len(), |at| index + at + 1)
            }
            ('/', Some('*')) => {
                let body = index + 2;
                let close = (body..chars.len().saturating_sub(1))
                    .find(|&at| chars[at] == '*' && chars[at + 1] == '/')?;
                // MySQL runs the code in `/*! */`. PostgreSQL nests comments.
                let inner: String = chars[body..close].iter().collect();
                if inner.starts_with('!') || inner.contains("/*") {
                    return None;
                }
                close + 2
            }
            _ => {
                code.push(char);
                index += 1;
                continue;
            }
        };
        code.push(' ');
        index = end;
    }
    Some(code)
}

/// The index after the `quote` that closes a string or name. A doubled quote stays inside.
/// `None` when it never closes.
fn close(chars: &[char], mut index: usize, quote: char) -> Option<usize> {
    while index < chars.len() {
        if chars[index] == quote {
            if chars.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        check_options, explainable, is_model_id, is_read_only, parse_reply, transaction_change,
    };
    use crate::model::AiOptions;

    #[test]
    fn only_known_options_pass() {
        let options = |model: Option<&str>, effort: Option<&str>| AiOptions {
            provider: "claude".to_owned(),
            model: model.map(str::to_owned),
            effort: effort.map(str::to_owned),
            read_only: false,
        };
        assert!(check_options(&options(None, None)).is_ok());
        assert!(check_options(&options(Some("opus"), Some("max"))).is_ok());
        assert!(check_options(&options(Some("claude-opus-5-5"), Some("low"))).is_ok());
        assert!(check_options(&options(Some("--dangerously-skip-permissions"), None)).is_err());
        assert!(check_options(&options(None, Some("huge"))).is_err());
        assert!(check_options(&options(None, Some(""))).is_err());
    }

    #[test]
    fn explain_takes_one_statement() {
        assert_eq!(explainable("SELECT 1;  "), Some("SELECT 1"));
        assert_eq!(
            explainable("/* x */ delete from t where a = ';'"),
            Some("/* x */ delete from t where a = ';'")
        );
        assert_eq!(explainable("SELECT 1; DELETE FROM t"), None);
        assert_eq!(explainable("ANALYZE DELETE FROM t"), None);
        assert_eq!(explainable("(ANALYZE) DELETE FROM t"), None);
        assert_eq!(explainable("DROP TABLE t"), None);
        assert_eq!(explainable("  "), None);
    }

    #[test]
    fn transaction_changes() {
        assert_eq!(transaction_change("SELECT 1"), None);
        assert_eq!(transaction_change("BEGIN"), Some(true));
        assert_eq!(
            transaction_change("start transaction; update t set a = 1"),
            Some(true)
        );
        assert_eq!(
            transaction_change("BEGIN; UPDATE t SET a = 1; COMMIT"),
            Some(false)
        );
        assert_eq!(transaction_change("ROLLBACK TO SAVEPOINT a"), None);
        assert_eq!(transaction_change("SET autocommit = 0"), Some(true));
        assert_eq!(transaction_change("SET autocommit = 1"), Some(false));
        assert_eq!(transaction_change("LOCK TABLES t WRITE"), Some(true));
        assert_eq!(transaction_change("UNLOCK TABLES"), Some(false));
        assert_eq!(transaction_change("SELECT 'BEGIN'"), None);
        // A procedure body: the `BEGIN` is not at the start of a statement.
        assert_eq!(
            transaction_change("CREATE PROCEDURE p() BEGIN SELECT 1; END"),
            None
        );
        // Text we cannot read for sure: any such word counts as open.
        assert_eq!(transaction_change("SELECT 1 # x\nBEGIN"), Some(true));
    }

    #[test]
    fn model_ids() {
        for good in [
            "opus",
            "fable",
            "claude-opus-5-5",
            "claude-sonnet-4-6",
            "claude-opus-5-5[1m]",
            "my.model-1",
            "4o",
            &"a".repeat(80),
        ] {
            assert!(is_model_id(good), "should pass: {good}");
        }
        for bad in [
            "",
            "-x",
            "--foo",
            "--dangerously-skip-permissions",
            "opus ",
            "claude opus",
            "a;rm -rf /",
            "a$(x)",
            "a|b",
            "Claude-Opus",
            "a\nb",
            ".hidden",
            "[1m]",
            &"a".repeat(81),
        ] {
            assert!(!is_model_id(bad), "should fail: {bad}");
        }
    }

    #[test]
    fn unknown_provider_is_rejected() {
        for provider in ["codex", "", "Claude", "claude --model x"] {
            let options = AiOptions {
                provider: provider.to_owned(),
                ..AiOptions::default()
            };
            let error = check_options(&options).expect_err(provider).to_string();
            assert!(error.contains("Unknown AI provider"), "{error}");
        }
    }

    #[test]
    fn reads() {
        for sql in [
            "SELECT * FROM users",
            "select count(*) from users;",
            "  SELECT 1 ;  ",
            "WITH a AS (SELECT 1) SELECT * FROM a",
            "SHOW TABLES",
            "EXPLAIN SELECT 1",
            "DESCRIBE users",
            "DESC users",
            "SELECT 'drop table x; delete' FROM t",
            "SELECT \"update\" FROM t",
            "SELECT [set] FROM t",
            "SELECT 1 -- delete everything\nFROM t",
            "SELECT /* insert */ 1",
            "SELECT 'it''s' FROM t",
            "SELECT updated_at, user_set FROM t",
        ] {
            assert!(is_read_only(sql), "should read: {sql}");
        }
    }

    #[test]
    fn writes() {
        for sql in [
            "",
            "   ",
            "DELETE FROM users",
            "UPDATE users SET blocked = 1 WHERE id = 3",
            "INSERT INTO t VALUES (1)",
            "SELECT 1; DELETE FROM t",
            "SELECT 1; SELECT 2",
            "SELECT * INTO backup FROM t",
            "SELECT * FROM t FOR UPDATE",
            "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d",
            "EXPLAIN ANALYZE DELETE FROM t",
            "SET @a = 1",
            "CALL p()",
            "EXEC sp_who",
            "TRUNCATE t",
            "DROP TABLE t",
            "VACUUM",
            "PRAGMA journal_mode = wal",
            "SELECT 'a\\'b'; DELETE FROM t; SELECT 'c'",
            "SELECT 1 # x'\n; DELETE FROM t; -- '",
            "SELECT 1--1; DELETE FROM t",
            "SELECT $$'$$; DELETE FROM t; SELECT $$'$$",
            "SELECT /*! 1; DELETE FROM t */",
            "SELECT /* a /* b */ ' */ ; DELETE FROM t; --'",
            "SELECT 'never closed",
            "SELECT /* never closed",
            "SELECT 1 /* x */ ; LOCK TABLES t WRITE",
        ] {
            assert!(!is_read_only(sql), "should be a write: {sql}");
        }
    }

    #[test]
    fn parses_replies() {
        let (sql, message) = parse_reply(r#"{"sql": "SELECT 1", "message": "Counting"}"#);
        assert_eq!(sql.as_deref(), Some("SELECT 1"));
        assert_eq!(message, "Counting");

        let fenced = "Here:\n```json\n{\"sql\": null, \"message\": \"There are 3.\"}\n```";
        assert_eq!(parse_reply(fenced), (None, "There are 3.".to_owned()));

        let blank = r#"{"sql": "  ", "message": "Done"}"#;
        assert_eq!(parse_reply(blank), (None, "Done".to_owned()));

        assert_eq!(parse_reply(" just text "), (None, "just text".to_owned()));
    }
}
