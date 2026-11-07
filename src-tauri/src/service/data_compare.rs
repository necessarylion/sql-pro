//! Data compare: which rows of one table differ between two databases, and the SQL
//! that makes the target rows the same as the source rows.
//!
//! `RowMerge` is pure. It takes pages of rows from both sides and finds the rows that
//! are only in the source, only in the target, or changed. The service reads the pages
//! and turns the differences into `DELETE`, `UPDATE` and `INSERT` statements.

use std::cmp::Ordering;
use std::collections::HashMap;

use super::ConnectionService;
use super::schema_compare::SchemaCompare;
use crate::constants::{
    MAX_DATA_DIFFS, MAX_QUERY_ROWS, OTHER_DATABASE, SYNC_SQL_CHANGED, TOO_MANY_DATA_DIFFS,
};
use crate::error::{AppError, AppResult};
use crate::model::{ColumnInfo, DataDiff, DataKinds, TableRef};
use crate::repository::{KeyCast, Repository, Statement};

/// One row as the app reads it: every value as text, NULL as `None`.
type Cells = Vec<Option<String>>;

/// The two sides of the compare, as index into the arrays of `RowMerge`.
const SOURCE: usize = 0;
const TARGET: usize = 1;

/// The rows that differ. A changed row is the source row with the indexes of the
/// columns that differ.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RowDiff {
    pub inserts: Vec<Cells>,
    pub updates: Vec<(Cells, Vec<usize>)>,
    pub deletes: Vec<Cells>,
    /// True when there are too many differences. Then the lists are empty.
    pub limited: bool,
    /// True when it gave up because too many rows waited, but few of them are surely
    /// different. Then the two sides likely sort the key in another order.
    pub unsorted: bool,
}

/// Merges the rows of both sides, read page by page in the order of the key.
///
/// A row waits until the other side brings the same key, or the other side ends.
/// So the result is right in any order. The side that is behind is read next, so
/// few rows wait: the side with fewer waiting rows, and on a tie the side with the
/// lower last key (numbers as numbers, else text without case).
pub struct RowMerge {
    /// The positions of the key columns in a row.
    key: Vec<usize>,
    /// The rows of a full page. A shorter page is the last one.
    page: usize,
    /// The most rows it may hold before it gives up.
    hold: usize,
    /// The rows of each side that wait for the other side, with their arrival number.
    waiting: [HashMap<Cells, (usize, Cells)>; 2],
    changed: Vec<(Cells, Vec<usize>)>,
    /// The key of the last row read on each side.
    last: [Option<Cells>; 2],
    done: [bool; 2],
    /// Counts every row read. A row's number is its place in the order of both sides.
    arrived: usize,
    /// Per side, the number of the newest row that found its match. When both sides
    /// sort the same way, a row of that side that still waits and came before it is
    /// surely a difference: the other side has gone past its key.
    passed: [usize; 2],
    limited: bool,
}

impl RowMerge {
    pub fn new(key: Vec<usize>, page: usize, hold: usize) -> Self {
        Self {
            key,
            page,
            hold,
            waiting: [HashMap::new(), HashMap::new()],
            changed: Vec::new(),
            last: [None, None],
            done: [false, false],
            arrived: 0,
            passed: [0, 0],
            limited: false,
        }
    }

    /// The side to read the next page from. `None` when both are read, or it gave up.
    pub fn next_side(&self) -> Option<usize> {
        if self.limited {
            return None;
        }
        match self.done {
            [true, true] => None,
            [true, false] => Some(TARGET),
            [false, true] => Some(SOURCE),
            [false, false] => {
                // The side with more waiting rows is ahead, so the other one is read.
                // When both wait the same, the key order decides.
                let waiting = self.waiting[TARGET].len().cmp(&self.waiting[SOURCE].len());
                let behind = waiting.then_with(|| {
                    Self::key_order(self.last[TARGET].as_ref(), self.last[SOURCE].as_ref())
                });
                Some(if behind == Ordering::Less {
                    TARGET
                } else {
                    SOURCE
                })
            }
        }
    }

    /// The key of the last row read on `side`. `None` before the first page.
    pub fn last_key(&self, side: usize) -> Option<&Cells> {
        self.last[side].as_ref()
    }

    /// Takes the next page of one side. A page shorter than a full one is its last.
    pub fn push_page(&mut self, side: usize, rows: Vec<Cells>) {
        if rows.len() < self.page {
            self.done[side] = true;
        }
        for row in rows {
            let key: Cells = self.key.iter().map(|&index| row[index].clone()).collect();
            self.last[side] = Some(key.clone());
            self.push(side, key, row);
        }
        let held = self.changed.len() + self.waiting[SOURCE].len() + self.waiting[TARGET].len();
        if held > self.hold {
            self.limited = true;
        }
    }

    fn push(&mut self, side: usize, key: Cells, row: Cells) {
        self.arrived += 1;
        let Some((number, found)) = self.waiting[1 - side].remove(&key) else {
            self.waiting[side].insert(key, (self.arrived, row));
            return;
        };
        self.passed[side] = self.arrived;
        self.passed[1 - side] = self.passed[1 - side].max(number);
        let (source, target) = if side == SOURCE {
            (row, found)
        } else {
            (found, row)
        };
        // NULL and '' are not the same: `None` is not `Some("")`.
        let changed: Vec<usize> = (0..source.len())
            .filter(|&index| source[index] != target[index])
            .collect();
        if !changed.is_empty() {
            self.changed.push((source, changed));
        }
    }

    /// The differences. With more than `max` of them, or when it gave up, the lists are empty.
    pub fn finish(self, max: usize) -> RowDiff {
        if self.limited {
            return RowDiff {
                limited: true,
                // With no match at all there is nothing to tell the sort order by.
                unsorted: self.passed != [0, 0] && self.sure() <= max,
                ..RowDiff::default()
            };
        }
        let [source, target] = self.waiting;
        if self.changed.len() + source.len() + target.len() > max {
            return RowDiff {
                limited: true,
                ..RowDiff::default()
            };
        }
        RowDiff {
            inserts: Self::in_order(source),
            updates: self.changed,
            deletes: Self::in_order(target),
            limited: false,
            unsorted: false,
        }
    }

    /// How many differences are sure, if both sides sort the key the same way.
    /// When the other side is read to its end, every waiting row is sure.
    fn sure(&self) -> usize {
        let mut sure = self.changed.len();
        for side in [SOURCE, TARGET] {
            for (number, _) in self.waiting[side].values() {
                if self.done[1 - side] || *number < self.passed[side] {
                    sure += 1;
                }
            }
        }
        sure
    }

    /// The waiting rows in the order they came, so the SQL is the same on each run.
    fn in_order(waiting: HashMap<Cells, (usize, Cells)>) -> Vec<Cells> {
        let mut rows: Vec<(usize, Cells)> = waiting.into_values().collect();
        rows.sort_unstable_by_key(|(arrived, _)| *arrived);
        rows.into_iter().map(|(_, row)| row).collect()
    }

    /// A key that is not read yet comes first.
    fn key_order(left: Option<&Cells>, right: Option<&Cells>) -> Ordering {
        let (Some(left), Some(right)) = (left, right) else {
            return left.is_some().cmp(&right.is_some());
        };
        for (left, right) in left.iter().zip(right) {
            let order = match (left, right) {
                (Some(left), Some(right)) => Self::value_order(left, right),
                _ => left.cmp(right),
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    }

    fn value_order(left: &str, right: &str) -> Ordering {
        if let (Ok(left), Ok(right)) = (left.parse::<i128>(), right.parse::<i128>()) {
            return left.cmp(&right);
        }
        if let (Ok(left), Ok(right)) = (left.parse::<f64>(), right.parse::<f64>())
            && let Some(order) = left.partial_cmp(&right)
        {
            return order;
        }
        left.to_lowercase()
            .cmp(&right.to_lowercase())
            .then_with(|| left.cmp(right))
    }
}

/// The columns of one table on both sides, as the compare uses them.
struct Columns {
    /// The columns both sides have, without the ones the database fills, in the order of
    /// the source.
    shared: Vec<(ColumnInfo, ColumnInfo)>,
    /// The positions of the key columns in `shared`.
    key: Vec<usize>,
    /// Per side, how each column of `shared` is read, and true when the value read is
    /// already a SQL literal (`Repository::compare_column`).
    read: [Vec<(String, bool)>; 2],
    /// Per side, how a key value is written (`Repository::key_casts`).
    casts: [HashMap<String, KeyCast>; 2],
}

impl Columns {
    /// The value of column `index` of a row of `side` as SQL for the target.
    fn value(
        &self,
        target: &dyn Repository,
        side: usize,
        index: usize,
        cell: Option<&str>,
    ) -> String {
        match cell {
            None => "NULL".to_owned(),
            Some(text) if self.read[side][index].1 => text.to_owned(),
            Some(text) => target.literal(text),
        }
    }

    /// A key value of a row of `side` as SQL that compares like the key column of
    /// `column_side`, where the SQL runs.
    fn key_value(
        &self,
        repository: &dyn Repository,
        column_side: usize,
        side: usize,
        index: usize,
        cell: Option<&str>,
    ) -> String {
        match cell {
            Some(text) if !self.read[side][index].1 => {
                let (source, target) = &self.shared[index];
                let name = if column_side == SOURCE {
                    &source.name
                } else {
                    &target.name
                };
                repository.key_literal(self.casts[column_side].get(name), text)
            }
            _ => self.value(repository, side, index, cell),
        }
    }

    /// True when the key of a row of `side` has a NULL. Such a row cannot be matched in SQL.
    fn null_key(&self, side: usize, row: &Cells) -> bool {
        self.key.iter().any(|&index| match &row[index] {
            None => true,
            Some(text) => self.read[side][index].1 && text == "NULL",
        })
    }
}

impl ConnectionService {
    /// Compares the rows of `table` in the databases of tab `source` and tab `target`,
    /// and makes the SQL that changes the target rows. It only reads. Nothing runs.
    pub async fn compare_data(
        &self,
        source: &str,
        target: &str,
        table: &TableRef,
    ) -> AppResult<DataDiff> {
        let from = self.connection(source)?;
        let to = self.connection(target)?;
        Self::same_kind(from.config.driver, to.config.driver)?;
        let (from, to) = (from.repository.as_ref(), to.repository.as_ref());
        let (source_columns, target_columns) =
            tokio::try_join!(from.columns(table), to.columns(table))?;
        let label = SchemaCompare::label(table);
        if source_columns.is_empty() || target_columns.is_empty() {
            return Err(AppError::Message(format!(
                "The table {label} must be in both databases."
            )));
        }
        let mut diff = DataDiff {
            database: Self::current_database(to).await?,
            ..DataDiff::default()
        };
        let mut columns = Self::shared_columns(
            &label,
            [from, to],
            [&source_columns, &target_columns],
            &mut diff,
        )?;
        let (source_casts, target_casts) =
            tokio::try_join!(from.key_casts(table), to.key_casts(table))?;
        columns.casts = [source_casts, target_casts];
        let rows = Self::merge_rows([from, to], table, &label, &columns).await?;
        if rows.limited {
            diff.limited = true;
            diff.unsorted = rows.unsorted;
            return Ok(diff);
        }
        Self::data_sql(to, table, &columns, &rows, &mut diff);
        Ok(diff)
    }

    /// Compares again and runs the SQL of the picked `kinds` on tab `target` in one
    /// transaction, so a failure keeps nothing. No SQL text comes from the frontend.
    /// It runs only when the new SQL is `reviewed`, the SQL the user saw, and the target
    /// tab still works on `database`.
    pub async fn run_data_sync(
        &self,
        source: &str,
        target: &str,
        table: &TableRef,
        kinds: DataKinds,
        reviewed: &[String],
        database: Option<&str>,
    ) -> AppResult<()> {
        let diff = self.compare_data(source, target, table).await?;
        if diff.database.as_deref() != database {
            return Err(OTHER_DATABASE.into());
        }
        if diff.limited {
            return Err(TOO_MANY_DATA_DIFFS.into());
        }
        let statements = Self::picked_sql(diff, kinds);
        if statements != reviewed {
            return Err(SYNC_SQL_CHANGED.into());
        }
        if statements.is_empty() {
            return Err("There is no SQL to run".into());
        }
        let mut script = Vec::with_capacity(statements.len());
        for (index, sql) in statements.into_iter().enumerate() {
            script.push(Statement {
                label: format!("Statement {}", index + 1),
                sql,
                params: Vec::new(),
            });
        }
        self.repository(target)?.run_script(script).await
    }

    /// The statements of the picked kinds, in the order they run: delete, update, insert.
    /// The frontend joins them the same way.
    fn picked_sql(diff: DataDiff, kinds: DataKinds) -> Vec<String> {
        let mut statements = Vec::new();
        if kinds.delete {
            statements.extend(diff.deletes);
        }
        if kinds.update {
            statements.extend(diff.updates);
        }
        if kinds.insert {
            statements.extend(diff.inserts);
        }
        statements
    }

    /// The columns both sides have. Fails when the primary keys are not the same.
    fn shared_columns(
        label: &str,
        repositories: [&dyn Repository; 2],
        [source, target]: [&[ColumnInfo]; 2],
        diff: &mut DataDiff,
    ) -> AppResult<Columns> {
        let source_key = Self::key_names(source);
        let target_key = Self::key_names(target);
        if source_key.is_empty() || target_key.is_empty() {
            return Err(AppError::Message(format!(
                "The table {label} has no primary key in the {}. \
                Rows can only be matched by a primary key.",
                if source_key.is_empty() {
                    "source"
                } else {
                    "target"
                }
            )));
        }
        if source_key != target_key {
            return Err(AppError::Message(format!(
                "The primary key of {label} is not the same: ({}) in the source, \
                ({}) in the target.",
                source_key.join(", "),
                target_key.join(", ")
            )));
        }
        let mut columns = Columns {
            shared: Vec::new(),
            key: Vec::new(),
            read: [Vec::new(), Vec::new()],
            casts: [HashMap::new(), HashMap::new()],
        };
        for wanted in source {
            let Some(found) = target.iter().find(|found| found.name == wanted.name) else {
                diff.source_only.push(wanted.name.clone());
                continue;
            };
            let filled = Self::filled(repositories[SOURCE], wanted)
                || Self::filled(repositories[TARGET], found);
            if filled {
                diff.generated.push(wanted.name.clone());
                continue;
            }
            if wanted.primary_key {
                columns.key.push(columns.shared.len());
                diff.key.push(wanted.name.clone());
            }
            diff.columns.push(wanted.name.clone());
            columns.read[SOURCE].push(repositories[SOURCE].compare_column(wanted));
            columns.read[TARGET].push(repositories[TARGET].compare_column(found));
            columns.shared.push((wanted.clone(), found.clone()));
        }
        for found in target {
            if !source.iter().any(|wanted| wanted.name == found.name) {
                diff.target_only.push(found.name.clone());
            }
        }
        if diff.key != source_key {
            return Err(AppError::Message(format!(
                "The primary key of {label} has a column the database fills itself. \
                It cannot be compared."
            )));
        }
        Ok(columns)
    }

    fn key_names(columns: &[ColumnInfo]) -> Vec<String> {
        columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect()
    }

    /// True for a column the database fills itself, which a row write cannot set:
    /// a generated column (`MySQL` writes `VIRTUAL GENERATED`), an identity column that
    /// is not part of the key (SQL Server refuses to change it), or a SQL Server
    /// `rowversion`. A `MySQL` default like `CURRENT_TIMESTAMP` is `DEFAULT_GENERATED`,
    /// a plain column.
    fn filled(repository: &dyn Repository, column: &ColumnInfo) -> bool {
        let extra = column.extra.to_ascii_lowercase();
        (extra.contains("generated") && !extra.contains("default_generated"))
            || (extra == "identity" && !column.primary_key)
            || repository.database_written(column)
    }

    /// Reads both sides in pages ordered by the key, and merges them in one pass.
    /// The next page starts after the last key read, so a deep page is as fast as the first.
    async fn merge_rows(
        sides: [&dyn Repository; 2],
        table: &TableRef,
        label: &str,
        columns: &Columns,
    ) -> AppResult<RowDiff> {
        // ponytail: a float key may not come back as the same text, so a page after it
        // could miss or repeat a row. Such a table pages with OFFSET, which is slower.
        let keyset = columns.key.iter().all(|&index| {
            let kind = columns.shared[index].0.data_type.to_ascii_lowercase();
            !["float", "double", "real"]
                .iter()
                .any(|word| kind.contains(word))
        });
        let mut selects = Vec::with_capacity(2);
        let mut keys = Vec::with_capacity(2);
        for (side, repository) in sides.iter().enumerate() {
            let list: Vec<&str> = columns.read[side]
                .iter()
                .map(|(select, _)| select.as_str())
                .collect();
            let names: Vec<String> = columns
                .key
                .iter()
                .map(|&index| repository.quote(&columns.shared[index].0.name))
                .collect();
            selects.push(format!(
                "SELECT {} FROM {}",
                list.join(", "),
                repository.table_name(table)
            ));
            keys.push(names);
        }
        // `query` keeps at most MAX_QUERY_ROWS rows, so one page is exactly that many.
        let mut merge = RowMerge::new(
            columns.key.clone(),
            MAX_QUERY_ROWS,
            2 * MAX_DATA_DIFFS + 2 * MAX_QUERY_ROWS,
        );
        let mut offsets = [0_u64; 2];
        while let Some(side) = merge.next_side() {
            let repository = sides[side];
            let ordering = format!(" ORDER BY {}", keys[side].join(", "));
            let after = match merge.last_key(side) {
                Some(last) if keyset => {
                    let mut values = Vec::with_capacity(last.len());
                    for (&index, cell) in columns.key.iter().zip(last) {
                        values.push(columns.key_value(
                            repository,
                            side,
                            side,
                            index,
                            cell.as_deref(),
                        ));
                    }
                    format!(" WHERE {}", Self::after_key(&keys[side], &values))
                }
                _ => String::new(),
            };
            let offset = if keyset { 0 } else { offsets[side] };
            let sql = format!(
                "{}{after}{}",
                selects[side],
                repository.page_sql(&ordering, MAX_QUERY_ROWS, offset)
            );
            let rows = repository.query(&sql).await?.pop().unwrap_or_default().rows;
            if rows.iter().any(|row| columns.null_key(side, row)) {
                return Err(AppError::Message(format!(
                    "The table {label} has a row with NULL in its primary key. \
                    Such a row cannot be matched."
                )));
            }
            offsets[side] += MAX_QUERY_ROWS as u64;
            merge.push_page(side, rows);
        }
        Ok(merge.finish(MAX_DATA_DIFFS))
    }

    /// The rows after a key in key order: `a > 1 OR (a = 1 AND b > 'x')`. It is spelled
    /// out, because SQL Server has no row value compare like `(a, b) > (1, 'x')`.
    fn after_key(names: &[String], values: &[String]) -> String {
        let mut parts = Vec::with_capacity(names.len());
        for last in 0..names.len() {
            let mut part = Vec::with_capacity(last + 1);
            for index in 0..last {
                part.push(format!("{} = {}", names[index], values[index]));
            }
            part.push(format!("{} > {}", names[last], values[last]));
            parts.push(format!("({})", part.join(" AND ")));
        }
        parts.join(" OR ")
    }

    /// Turns the row differences into SQL for the target.
    fn data_sql(
        target: &dyn Repository,
        table: &TableRef,
        columns: &Columns,
        rows: &RowDiff,
        diff: &mut DataDiff,
    ) {
        let name = target.table_name(table);
        let mut names = Vec::with_capacity(columns.shared.len());
        for (column, _) in &columns.shared {
            names.push(target.quote(&column.name));
        }
        diff.delete_rows = rows.deletes.len();
        for row in &rows.deletes {
            diff.deletes.push(format!(
                "DELETE FROM {name} WHERE {}",
                Self::key_match(target, columns, &names, TARGET, row)
            ));
        }
        diff.update_rows = rows.updates.len();
        for (row, changed) in &rows.updates {
            let mut sets = Vec::with_capacity(changed.len());
            for &index in changed {
                sets.push(format!(
                    "{} = {}",
                    names[index],
                    columns.value(target, SOURCE, index, row[index].as_deref())
                ));
            }
            diff.updates.push(format!(
                "UPDATE {name} SET {} WHERE {}",
                sets.join(", "),
                Self::key_match(target, columns, &names, SOURCE, row)
            ));
        }
        diff.insert_rows = rows.inserts.len();
        if rows.inserts.is_empty() {
            return;
        }
        // An identity key gets the value of the source, so the database must allow it.
        let identity = columns
            .shared
            .iter()
            .any(|(_, column)| column.extra == "identity");
        let words = if identity {
            target.identity_override()
        } else {
            ""
        };
        let start = format!("INSERT INTO {name} ({}) {words}VALUES (", names.join(", "));
        let allow = identity
            .then(|| target.identity_insert_sql(table, true))
            .flatten();
        if let Some(sql) = allow {
            diff.inserts.push(sql);
        }
        for row in &rows.inserts {
            let mut values = Vec::with_capacity(row.len());
            for (index, cell) in row.iter().enumerate() {
                values.push(columns.value(target, SOURCE, index, cell.as_deref()));
            }
            diff.inserts.push(format!("{start}{})", values.join(", ")));
        }
        if let Some(sql) = identity
            .then(|| target.identity_insert_sql(table, false))
            .flatten()
        {
            diff.inserts.push(sql);
        }
    }

    /// `a = 1 AND b = 'x'` for the key of a row of `side`. A key value is never NULL.
    fn key_match(
        target: &dyn Repository,
        columns: &Columns,
        names: &[String],
        side: usize,
        row: &Cells,
    ) -> String {
        let mut parts = Vec::with_capacity(columns.key.len());
        for &index in &columns.key {
            parts.push(format!(
                "{} = {}",
                names[index],
                columns.key_value(target, TARGET, side, index, row[index].as_deref())
            ));
        }
        parts.join(" AND ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[Option<&str>]) -> Cells {
        cells.iter().map(|cell| cell.map(str::to_owned)).collect()
    }

    /// Feeds two sides into a merge, in pages of `page` rows, the way the service does.
    fn merge(source: &[Cells], target: &[Cells], key: Vec<usize>, page: usize) -> RowDiff {
        let mut merge = RowMerge::new(key, page, 1000);
        let sides = [source, target];
        let mut offsets = [0; 2];
        while let Some(side) = merge.next_side() {
            let rows = sides[side];
            let start = offsets[side].min(rows.len());
            let end = (start + page).min(rows.len());
            merge.push_page(side, rows[start..end].to_vec());
            offsets[side] += page;
        }
        merge.finish(100)
    }

    #[test]
    fn finds_rows_only_in_source_only_in_target_and_changed() {
        let source = [
            row(&[Some("1"), Some("a")]),
            row(&[Some("2"), Some("b")]),
            row(&[Some("4"), Some("d")]),
        ];
        let target = [
            row(&[Some("1"), Some("a")]),
            row(&[Some("2"), Some("B")]),
            row(&[Some("3"), Some("c")]),
        ];
        let diff = merge(&source, &target, vec![0], 2);
        assert_eq!(diff.inserts, vec![row(&[Some("4"), Some("d")])]);
        assert_eq!(diff.deletes, vec![row(&[Some("3"), Some("c")])]);
        assert_eq!(diff.updates, vec![(row(&[Some("2"), Some("b")]), vec![1])]);
        assert!(!diff.limited);
    }

    #[test]
    fn null_is_not_an_empty_text() {
        let source = [row(&[Some("1"), None]), row(&[Some("2"), Some("")])];
        let target = [row(&[Some("1"), Some("")]), row(&[Some("2"), None])];
        let diff = merge(&source, &target, vec![0], 10);
        assert_eq!(
            diff.updates,
            vec![
                (row(&[Some("1"), None]), vec![1]),
                (row(&[Some("2"), Some("")]), vec![1])
            ]
        );
        let same = merge(&source, &source, vec![0], 10);
        assert_eq!(same, RowDiff::default());
    }

    #[test]
    fn composite_keys_match_on_all_parts() {
        let source = [
            row(&[Some("x"), Some("1"), Some("a")]),
            row(&[Some("x"), Some("2"), Some("b")]),
        ];
        let target = [
            row(&[Some("x"), Some("1"), Some("z")]),
            row(&[Some("y"), Some("2"), Some("b")]),
        ];
        // The key is the first two columns, in the middle of no other column.
        let diff = merge(&source, &target, vec![0, 1], 1);
        assert_eq!(
            diff.updates,
            vec![(row(&[Some("x"), Some("1"), Some("a")]), vec![2])]
        );
        assert_eq!(diff.inserts, vec![row(&[Some("x"), Some("2"), Some("b")])]);
        assert_eq!(diff.deletes, vec![row(&[Some("y"), Some("2"), Some("b")])]);
    }

    #[test]
    fn rows_across_a_page_edge_still_match() {
        // Exactly two full pages on one side, so the last read is an empty page.
        // The other side is shifted by one row, so matches fall on other pages.
        let mut source = Vec::new();
        let mut target = Vec::new();
        for id in 1..=8 {
            source.push(row(&[Some(&id.to_string()), Some("v")]));
        }
        for id in 2..=10 {
            target.push(row(&[Some(&id.to_string()), Some("v")]));
        }
        let diff = merge(&source, &target, vec![0], 4);
        assert_eq!(diff.inserts, vec![row(&[Some("1"), Some("v")])]);
        assert_eq!(
            diff.deletes,
            vec![row(&[Some("9"), Some("v")]), row(&[Some("10"), Some("v")])]
        );
        assert_eq!(diff.updates.len(), 0);
    }

    #[test]
    fn numbers_sort_as_numbers_so_few_rows_wait() {
        let order = RowMerge::key_order(Some(&row(&[Some("9")])), Some(&row(&[Some("10")])));
        assert_eq!(order, Ordering::Less);
        let order = RowMerge::key_order(Some(&row(&[Some("b")])), Some(&row(&[Some("A")])));
        assert_eq!(order, Ordering::Greater);
        assert_eq!(RowMerge::key_order(None, Some(&row(&[]))), Ordering::Less);
    }

    #[test]
    fn a_key_order_it_cannot_guess_still_holds_few_rows() {
        // The database sorts the keys from high to low, like a SQL Server GUID that the
        // text order gets wrong. Two equal sides must not read far ahead of each other.
        let mut rows = Vec::new();
        for id in (1..=20).rev() {
            rows.push(row(&[Some(&id.to_string())]));
        }
        let mut merge = RowMerge::new(vec![0], 2, 4);
        let mut offsets = [0; 2];
        while let Some(side) = merge.next_side() {
            let start = offsets[side].min(rows.len());
            let end = (start + 2).min(rows.len());
            merge.push_page(side, rows[start..end].to_vec());
            offsets[side] += 2;
        }
        assert_eq!(merge.finish(100), RowDiff::default());
    }

    #[test]
    fn the_next_page_starts_after_the_last_key() {
        let names = ["a".to_owned(), "b".to_owned()];
        let values = ["1".to_owned(), "'x'".to_owned()];
        assert_eq!(
            ConnectionService::after_key(&names, &values),
            "(a > 1) OR (a = 1 AND b > 'x')"
        );
    }

    #[test]
    fn sql_runs_delete_then_update_then_insert_of_the_picked_kinds() {
        let diff = DataDiff {
            deletes: vec!["d".to_owned()],
            updates: vec!["u".to_owned()],
            inserts: vec!["i1".to_owned(), "i2".to_owned()],
            ..DataDiff::default()
        };
        let all = DataKinds {
            insert: true,
            update: true,
            delete: true,
        };
        assert_eq!(
            ConnectionService::picked_sql(diff, all),
            ["d", "u", "i1", "i2"]
        );
        let diff = DataDiff {
            deletes: vec!["d".to_owned()],
            updates: vec!["u".to_owned()],
            inserts: vec!["i".to_owned()],
            ..DataDiff::default()
        };
        let no_delete = DataKinds {
            delete: false,
            ..all
        };
        assert_eq!(ConnectionService::picked_sql(diff, no_delete), ["u", "i"]);
    }

    #[test]
    fn too_many_differences_give_no_rows() {
        let mut source = Vec::new();
        for id in 0..20 {
            source.push(row(&[Some(&id.to_string())]));
        }
        let mut merge = RowMerge::new(vec![0], 100, 1000);
        merge.push_page(SOURCE, source);
        merge.push_page(TARGET, Vec::new());
        let diff = merge.finish(10);
        assert!(diff.limited);
        assert_eq!(diff.inserts.len(), 0);

        // Holding too many rows on the way stops the reads at once.
        let mut merge = RowMerge::new(vec![0], 2, 3);
        merge.push_page(SOURCE, vec![row(&[Some("1")]), row(&[Some("2")])]);
        assert_eq!(merge.next_side(), Some(TARGET));
        merge.push_page(TARGET, vec![row(&[Some("3")]), row(&[Some("4")])]);
        assert_eq!(merge.next_side(), None);
        assert!(merge.finish(100).limited);
    }

    /// Runs a merge with the real page size and limits, the way the service does.
    fn full_merge(source: &[Cells], target: &[Cells]) -> RowDiff {
        let mut merge = RowMerge::new(
            vec![0],
            MAX_QUERY_ROWS,
            2 * MAX_DATA_DIFFS + 2 * MAX_QUERY_ROWS,
        );
        let sides = [source, target];
        let mut offsets = [0; 2];
        while let Some(side) = merge.next_side() {
            let rows = sides[side];
            let start = offsets[side].min(rows.len());
            let end = (start + MAX_QUERY_ROWS).min(rows.len());
            merge.push_page(side, rows[start..end].to_vec());
            offsets[side] += MAX_QUERY_ROWS;
        }
        merge.finish(MAX_DATA_DIFFS)
    }

    fn numbered(ids: std::ops::Range<u32>) -> Vec<Cells> {
        ids.map(|id| row(&[Some(&id.to_string())])).collect()
    }

    #[test]
    fn many_real_differences_are_not_a_sort_problem() {
        let cases = [
            (numbered(1..25_001), Vec::new()),
            (numbered(1..50_001), numbered(1..1001)),
            (numbered(1..25_001), numbered(100_001..125_001)),
        ];
        for (source, target) in cases {
            let diff = full_merge(&source, &target);
            assert!(diff.limited);
            assert!(!diff.unsorted, "{} / {}", source.len(), target.len());
        }
    }

    #[test]
    fn sides_that_sort_the_key_apart_are_named() {
        // The same 20 keys. The target gives the first two the same way, then the rest
        // from high to low.
        let mut source = Vec::new();
        for id in 1..=20 {
            source.push(row(&[Some(&id.to_string())]));
        }
        let mut target = source.clone();
        target[2..].reverse();
        let mut merge = RowMerge::new(vec![0], 2, 8);
        let mut offsets = [0; 2];
        let sides = [&source, &target];
        while let Some(side) = merge.next_side() {
            let start = offsets[side].min(20);
            merge.push_page(side, sides[side][start..(start + 2).min(20)].to_vec());
            offsets[side] += 2;
        }
        let diff = merge.finish(5);
        assert!(diff.limited);
        assert!(diff.unsorted);

        // Real differences on both sides, in the same order: many are sure.
        let mut merge = RowMerge::new(vec![0], 4, 8);
        let keys = |from: u32| -> Vec<Cells> {
            (from..from + 4)
                .map(|id| row(&[Some(&format!("{id:03}"))]))
                .collect()
        };
        merge.push_page(
            SOURCE,
            [vec![row(&[Some("000")])], keys(100)[..3].to_vec()].concat(),
        );
        merge.push_page(
            TARGET,
            [vec![row(&[Some("000")])], keys(200)[..3].to_vec()].concat(),
        );
        merge.push_page(
            SOURCE,
            [vec![row(&[Some("300")])], keys(103)[..3].to_vec()].concat(),
        );
        merge.push_page(TARGET, vec![row(&[Some("300")]), row(&[Some("400")])]);
        merge.push_page(SOURCE, vec![row(&[Some("400")]), row(&[Some("500")])]);
        let diff = merge.finish(5);
        assert!(diff.limited);
        assert!(!diff.unsorted);
    }
}
