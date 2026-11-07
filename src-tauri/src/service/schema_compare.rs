//! Schema compare: what differs between two databases, and the SQL that makes the
//! target the same as the source.
//!
//! `SchemaCompare` is pure. It takes two snapshots and gives the differences. Then it
//! puts the picked differences into steps in a safe order. The service reads the
//! snapshots and turns each step into SQL with the same helpers the Structure view uses.

use std::fmt::Write as _;

use super::ConnectionService;
use super::claude::dialect;
use super::export::Exporter;
use crate::constants::{
    COLUMN_RULE_CHANGED, COLUMN_RULE_NOT_COPIED, EXPRESSION_INDEX, OTHER_DATABASE, PARTIAL_INDEX,
    SQLITE_AUTO_INDEX, SYNC_SQL_CHANGED, TABLE_NOT_COPIED, UNNAMED_FOREIGN_KEY, VIEW,
};
use crate::error::{AppError, AppResult};
use crate::model::{
    ChangeAction, ColumnInfo, ColumnSpec, Driver, ForeignKey, IndexInfo, SchemaChange, SyncSql,
    TableRef,
};
use crate::repository::{Repository, Statement};

/// One table with everything the compare looks at.
#[derive(Clone, Debug)]
pub struct TableSchema {
    pub table: TableRef,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKey>,
}

/// One difference, with what the SQL needs. Old parts come from the target, new parts
/// from the source.
#[derive(Clone, Debug)]
enum Diff {
    CreateTable(TableSchema),
    DropTable(TableSchema),
    AddColumn(ColumnInfo),
    DropColumn(ColumnInfo),
    AlterColumn { old: ColumnInfo, new: ColumnInfo },
    AddIndex(IndexInfo),
    DropIndex(IndexInfo),
    AlterIndex { old: IndexInfo, new: IndexInfo },
    AddForeignKey(ForeignKey),
    DropForeignKey(ForeignKey),
}

impl Diff {
    fn action(&self) -> ChangeAction {
        match self {
            Self::CreateTable(_) => ChangeAction::CreateTable,
            Self::DropTable(_) => ChangeAction::DropTable,
            Self::AddColumn(_) => ChangeAction::AddColumn,
            Self::DropColumn(_) => ChangeAction::DropColumn,
            Self::AlterColumn { .. } => ChangeAction::AlterColumn,
            Self::AddIndex(_) => ChangeAction::AddIndex,
            Self::DropIndex(_) => ChangeAction::DropIndex,
            Self::AlterIndex { .. } => ChangeAction::AlterIndex,
            Self::AddForeignKey(_) => ChangeAction::AddForeignKey,
            Self::DropForeignKey(_) => ChangeAction::DropForeignKey,
        }
    }
}

/// One difference: what the frontend shows, and the data to make its SQL.
#[derive(Clone, Debug)]
pub struct Change {
    pub info: SchemaChange,
    diff: Diff,
}

/// One change of the target. The variants are in the order they must run:
/// foreign keys go first, so nothing that is dropped is still used by one.
/// New foreign keys go last, so the tables and columns they need are there.
#[derive(Clone, Debug)]
pub enum Step {
    DropForeignKey(TableRef, ForeignKey),
    DropIndex(TableRef, IndexInfo),
    DropTable(TableRef),
    DropColumn(TableRef, String),
    CreateTable(TableSchema),
    AddColumn(TableRef, ColumnInfo),
    /// The column as the target has it now, and as the source has it.
    AlterColumn(TableRef, ColumnInfo, ColumnInfo),
    AddIndex(TableRef, IndexInfo),
    AddForeignKey(TableRef, ForeignKey),
}

impl Step {
    /// Steps run from the lowest phase to the highest.
    fn phase(&self) -> u8 {
        match self {
            Self::DropForeignKey(..) => 0,
            Self::DropIndex(..) => 1,
            Self::DropTable(..) => 2,
            Self::DropColumn(..) => 3,
            Self::CreateTable(..) => 4,
            Self::AddColumn(..) => 5,
            Self::AlterColumn(..) => 6,
            Self::AddIndex(..) => 7,
            Self::AddForeignKey(..) => 8,
        }
    }

    /// What a warning calls this step.
    fn label(&self) -> String {
        match self {
            Self::DropForeignKey(table, key) => format!(
                "Drop foreign key {} of {}",
                SchemaCompare::key_text(key),
                SchemaCompare::label(table)
            ),
            Self::DropIndex(table, index) => {
                format!(
                    "Drop index {} of {}",
                    index.name,
                    SchemaCompare::label(table)
                )
            }
            Self::DropTable(table) => format!("Drop table {}", SchemaCompare::label(table)),
            Self::DropColumn(table, name) => {
                format!("Drop column {name} of {}", SchemaCompare::label(table))
            }
            Self::CreateTable(schema) => {
                format!("Create table {}", SchemaCompare::label(&schema.table))
            }
            Self::AddColumn(table, column) => {
                format!(
                    "Add column {} to {}",
                    column.name,
                    SchemaCompare::label(table)
                )
            }
            Self::AlterColumn(table, old, _) => {
                format!(
                    "Change column {} of {}",
                    old.name,
                    SchemaCompare::label(table)
                )
            }
            Self::AddIndex(table, index) => {
                format!(
                    "Add index {} to {}",
                    index.name,
                    SchemaCompare::label(table)
                )
            }
            Self::AddForeignKey(table, key) => format!(
                "Add foreign key {} to {}",
                SchemaCompare::key_text(key),
                SchemaCompare::label(table)
            ),
        }
    }
}

/// What the target database can do. It decides the steps a missing table needs.
#[derive(Clone, Copy, Debug)]
pub struct SyncRules {
    /// False when a foreign key of an existing table cannot be added or dropped (`SQLite`).
    pub alter_foreign_keys: bool,
    /// True when the `CREATE TABLE` of the source holds its indexes too (`MySQL`).
    pub create_has_indexes: bool,
    /// True when the `CREATE TABLE` of the source holds its foreign keys (`MySQL`, `SQLite`).
    pub create_has_foreign_keys: bool,
}

impl SyncRules {
    /// `MySQL` and `SQLite` give the `CREATE TABLE` as they keep it. For the others the
    /// app builds it from the columns, so the indexes and foreign keys come after it.
    pub fn of(driver: Driver) -> Self {
        match driver {
            Driver::Mysql => Self {
                alter_foreign_keys: true,
                create_has_indexes: true,
                create_has_foreign_keys: true,
            },
            Driver::Sqlite => Self {
                alter_foreign_keys: false,
                create_has_indexes: false,
                create_has_foreign_keys: true,
            },
            Driver::Postgres | Driver::Mssql => Self {
                alter_foreign_keys: true,
                create_has_indexes: false,
                create_has_foreign_keys: false,
            },
        }
    }
}

/// Compares two schema snapshots and orders the changes. No database access.
// ponytail: tables, columns, indexes and foreign keys only. Views, triggers, routines,
// a changed primary key and a renamed column (it shows as drop + add) are left out.
pub struct SchemaCompare;

impl SchemaCompare {
    /// What must change in `target` to make it the same as `source`, sorted by table.
    pub fn diff(source: &[TableSchema], target: &[TableSchema]) -> Vec<Change> {
        let mut changes = Vec::new();
        for wanted in source {
            match Self::find(target, &wanted.table) {
                Some(found) => Self::table_diff(wanted, found, &mut changes),
                None => changes.push(Self::change(
                    &wanted.table,
                    "",
                    "",
                    Self::count(wanted.columns.len()),
                    Diff::CreateTable(wanted.clone()),
                )),
            }
        }
        for found in target {
            if Self::find(source, &found.table).is_none() {
                changes.push(Self::change(
                    &found.table,
                    "",
                    "",
                    Self::count(found.columns.len()),
                    Diff::DropTable(found.clone()),
                ));
            }
        }
        // A stable sort, so the changes of one table keep their order.
        changes.sort_by_cached_key(|change| Self::label(&change.info.table));
        changes
    }

    /// The steps of the picked changes, in the order they must run.
    pub fn plan(changes: &[&Change], rules: SyncRules) -> Vec<Step> {
        let mut steps = Vec::new();
        let mut created = Vec::new();
        for change in changes {
            let table = &change.info.table;
            match &change.diff {
                Diff::CreateTable(schema) if !Self::can_create(schema, rules) => {}
                Diff::CreateTable(schema) => created.push(schema),
                Diff::DropTable(schema) => {
                    // Its foreign keys go first. Two dropped tables may point at each other.
                    if rules.alter_foreign_keys {
                        for key in &schema.foreign_keys {
                            steps.push(Step::DropForeignKey(table.clone(), key.clone()));
                        }
                    }
                    steps.push(Step::DropTable(table.clone()));
                }
                Diff::AddColumn(column) => {
                    steps.push(Step::AddColumn(table.clone(), column.clone()));
                }
                Diff::DropColumn(column) => {
                    steps.push(Step::DropColumn(table.clone(), column.name.clone()));
                }
                Diff::AlterColumn { old, new } => {
                    steps.push(Step::AlterColumn(table.clone(), old.clone(), new.clone()));
                }
                Diff::AddIndex(index) => steps.push(Step::AddIndex(table.clone(), index.clone())),
                Diff::DropIndex(index) => steps.push(Step::DropIndex(table.clone(), index.clone())),
                Diff::AlterIndex { old, new } => {
                    // The new one cannot be made in SQL here. So the old one stays too,
                    // and only the warning of the new one shows.
                    if Self::can_make(new) {
                        steps.push(Step::DropIndex(table.clone(), old.clone()));
                    }
                    steps.push(Step::AddIndex(table.clone(), new.clone()));
                }
                Diff::AddForeignKey(key) => {
                    steps.push(Step::AddForeignKey(table.clone(), key.clone()));
                }
                Diff::DropForeignKey(key) => {
                    steps.push(Step::DropForeignKey(table.clone(), key.clone()));
                }
            }
        }
        for schema in Self::creation_order(created) {
            steps.push(Step::CreateTable(schema.clone()));
            if !rules.create_has_indexes {
                for index in Self::indexes(&schema.indexes) {
                    steps.push(Step::AddIndex(schema.table.clone(), index.clone()));
                }
            }
            if !rules.create_has_foreign_keys {
                for key in &schema.foreign_keys {
                    steps.push(Step::AddForeignKey(schema.table.clone(), key.clone()));
                }
            }
        }
        // A stable sort, so the new tables keep the order of `creation_order`.
        steps.sort_by_key(Step::phase);
        steps
    }

    /// The warnings of the picked changes that `plan` leaves out, because no SQL can do them.
    /// A new table that cannot be made also takes its indexes and foreign keys with it.
    pub fn skipped(changes: &[&Change], rules: SyncRules) -> Vec<String> {
        let mut warnings = Vec::new();
        for change in changes {
            if let Diff::CreateTable(schema) = &change.diff
                && !Self::can_create(schema, rules)
            {
                warnings.push(format!(
                    "Create table {}: {TABLE_NOT_COPIED}",
                    Self::label(&schema.table)
                ));
            }
        }
        warnings
    }

    /// False when the app would build the `CREATE TABLE` and the table has a generated
    /// column. The build would write its expression as a default, which is wrong.
    /// The app builds it exactly where the database's own `CREATE TABLE` (with the
    /// foreign keys inside) is not there: `PostgreSQL` and SQL Server.
    fn can_create(schema: &TableSchema, rules: SyncRules) -> bool {
        rules.create_has_foreign_keys
            || !schema
                .columns
                .iter()
                .any(|column| column.extra == "generated")
    }

    /// True when SQL can make the index: the database gave its full `CREATE INDEX`, or it
    /// has only plain columns and no `WHERE` part.
    fn can_make(index: &IndexInfo) -> bool {
        index.definition.is_some() || (!index.expression && !index.partial)
    }

    /// The new tables, with a table before the new tables whose foreign keys point at it.
    /// A `CREATE TABLE` with its foreign keys inside (`MySQL`) needs that order.
    // ponytail: tables that point at each other in a circle keep the source order.
    // Then `MySQL` refuses one of them. Add the keys after the tables if that bites.
    fn creation_order(mut left: Vec<&TableSchema>) -> Vec<&TableSchema> {
        let mut ordered = Vec::with_capacity(left.len());
        while !left.is_empty() {
            let ready = left.iter().position(|schema| {
                schema.foreign_keys.iter().all(|key| {
                    key.ref_table == schema.table.name
                        || !left.iter().any(|other| other.table.name == key.ref_table)
                })
            });
            ordered.push(left.remove(ready.unwrap_or(0)));
        }
        ordered
    }

    fn table_diff(source: &TableSchema, target: &TableSchema, changes: &mut Vec<Change>) {
        Self::columns_diff(source, target, changes);
        Self::indexes_diff(source, target, changes);
        Self::keys_diff(source, target, changes);
    }

    fn columns_diff(source: &TableSchema, target: &TableSchema, changes: &mut Vec<Change>) {
        let table = &target.table;
        for wanted in &source.columns {
            match target
                .columns
                .iter()
                .find(|found| found.name == wanted.name)
            {
                None => changes.push(Self::change(
                    table,
                    &wanted.name,
                    &wanted.name,
                    Self::column_text(wanted),
                    Diff::AddColumn(wanted.clone()),
                )),
                Some(found) if !Self::same_column(wanted, found) => changes.push(Self::change(
                    table,
                    &wanted.name,
                    &wanted.name,
                    format!(
                        "{} → {}",
                        Self::column_text(found),
                        Self::column_text(wanted)
                    ),
                    Diff::AlterColumn {
                        old: found.clone(),
                        new: wanted.clone(),
                    },
                )),
                Some(_) => {}
            }
        }
        for found in &target.columns {
            if !source
                .columns
                .iter()
                .any(|wanted| wanted.name == found.name)
            {
                changes.push(Self::change(
                    table,
                    &found.name,
                    &found.name,
                    Self::column_text(found),
                    Diff::DropColumn(found.clone()),
                ));
            }
        }
    }

    fn indexes_diff(source: &TableSchema, target: &TableSchema, changes: &mut Vec<Change>) {
        let table = &target.table;
        let wanted_indexes = Self::indexes(&source.indexes);
        let found_indexes = Self::indexes(&target.indexes);
        for wanted in &wanted_indexes {
            match found_indexes.iter().find(|found| found.name == wanted.name) {
                None => changes.push(Self::change(
                    table,
                    &wanted.name,
                    &wanted.name,
                    Self::index_text(wanted),
                    Diff::AddIndex((*wanted).clone()),
                )),
                Some(found)
                    if found.columns != wanted.columns
                        || found.unique != wanted.unique
                        || found.definition != wanted.definition =>
                {
                    changes.push(Self::change(
                        table,
                        &wanted.name,
                        &wanted.name,
                        format!("{} → {}", Self::index_text(found), Self::index_text(wanted)),
                        Diff::AlterIndex {
                            old: (*found).clone(),
                            new: (*wanted).clone(),
                        },
                    ));
                }
                Some(_) => {}
            }
        }
        for found in &found_indexes {
            if !wanted_indexes
                .iter()
                .any(|wanted| wanted.name == found.name)
            {
                changes.push(Self::change(
                    table,
                    &found.name,
                    &found.name,
                    Self::index_text(found),
                    Diff::DropIndex((*found).clone()),
                ));
            }
        }
    }

    /// A foreign key is matched by what it does, not by its name. `SQLite` keeps no names.
    fn keys_diff(source: &TableSchema, target: &TableSchema, changes: &mut Vec<Change>) {
        let table = &target.table;
        for wanted in &source.foreign_keys {
            if !target
                .foreign_keys
                .iter()
                .any(|found| Self::same_key(found, wanted))
            {
                changes.push(Self::change(
                    table,
                    &wanted.name,
                    &Self::key_text(wanted),
                    Self::key_text(wanted),
                    Diff::AddForeignKey(wanted.clone()),
                ));
            }
        }
        for found in &target.foreign_keys {
            if !source
                .foreign_keys
                .iter()
                .any(|wanted| Self::same_key(found, wanted))
            {
                changes.push(Self::change(
                    table,
                    &found.name,
                    &Self::key_text(found),
                    Self::key_text(found),
                    Diff::DropForeignKey(found.clone()),
                ));
            }
        }
    }

    /// `key` makes the id unique inside the table. It is the name, or what a foreign key does.
    fn change(table: &TableRef, name: &str, key: &str, detail: String, diff: Diff) -> Change {
        let action = diff.action();
        Change {
            info: SchemaChange {
                id: format!("{action:?}:{}:{key}", Self::label(table)),
                table: table.clone(),
                action,
                name: name.to_owned(),
                detail,
            },
            diff,
        }
    }

    /// The table with the same schema and name.
    fn find<'list>(list: &'list [TableSchema], table: &TableRef) -> Option<&'list TableSchema> {
        list.iter()
            .find(|schema| schema.table.name == table.name && schema.table.schema == table.schema)
    }

    /// The indexes the compare looks at: no primary key, and none `SQLite` made by itself.
    fn indexes(list: &[IndexInfo]) -> Vec<&IndexInfo> {
        list.iter()
            .filter(|index| {
                !index.primary
                    && !index.name.is_empty()
                    && !index.name.starts_with(SQLITE_AUTO_INDEX)
            })
            .collect()
    }

    /// Both databases are of the same kind, so the texts can be compared as they are.
    /// Only the case of a type is not looked at: `SQLite` keeps the type as it was typed.
    fn same_column(left: &ColumnInfo, right: &ColumnInfo) -> bool {
        left.data_type.eq_ignore_ascii_case(&right.data_type)
            && left.nullable == right.nullable
            && left.default == right.default
            && left.extra.eq_ignore_ascii_case(&right.extra)
    }

    /// True when the column numbers its rows itself, like `AUTO_INCREMENT` or `IDENTITY`.
    fn numbers_itself(column: &ColumnInfo) -> bool {
        column.extra.eq_ignore_ascii_case("auto_increment") || column.extra == "identity"
    }

    /// True when the extra rule is one that a plain column definition already holds.
    fn plain_extra(column: &ColumnInfo) -> bool {
        column.extra.is_empty() || column.extra.eq_ignore_ascii_case("DEFAULT_GENERATED")
    }

    fn same_key(left: &ForeignKey, right: &ForeignKey) -> bool {
        left.columns == right.columns
            && left.ref_schema == right.ref_schema
            && left.ref_table == right.ref_table
            && left.ref_columns == right.ref_columns
            && left.on_delete == right.on_delete
            && left.on_update == right.on_update
    }

    /// The name of a table, with its schema in front when it has one.
    pub fn label(table: &TableRef) -> String {
        match &table.schema {
            Some(schema) => format!("{schema}.{}", table.name),
            None => table.name.clone(),
        }
    }

    fn count(columns: usize) -> String {
        format!(
            "{columns} {}",
            if columns == 1 { "column" } else { "columns" }
        )
    }

    /// Like `int NOT NULL DEFAULT 0`.
    fn column_text(column: &ColumnInfo) -> String {
        let mut text = column.data_type.clone();
        if !column.nullable {
            text.push_str(" NOT NULL");
        }
        if let Some(default) = &column.default {
            text.push_str(" DEFAULT ");
            text.push_str(default);
        }
        if !Self::plain_extra(column) {
            text.push(' ');
            text.push_str(&column.extra);
        }
        text
    }

    /// Like `UNIQUE (a, b)`.
    /// The full `CREATE INDEX` when the database gives one.
    fn index_text(index: &IndexInfo) -> String {
        if let Some(definition) = &index.definition {
            return definition.clone();
        }
        let unique = if index.unique { "UNIQUE " } else { "" };
        format!("{unique}({})", index.columns.join(", "))
    }

    /// Like `(user_id) → users (id) ON DELETE CASCADE`. The rules show only when they
    /// are not the default.
    fn key_text(key: &ForeignKey) -> String {
        let target = match &key.ref_schema {
            Some(schema) => format!("{schema}.{}", key.ref_table),
            None => key.ref_table.clone(),
        };
        let mut text = format!(
            "({}) → {target} ({})",
            key.columns.join(", "),
            key.ref_columns.join(", ")
        );
        for (rule, value) in [("ON DELETE", &key.on_delete), ("ON UPDATE", &key.on_update)] {
            if !value.is_empty() && value != "NO ACTION" {
                let _ = write!(text, " {rule} {value}");
            }
        }
        text
    }
}

impl ConnectionService {
    /// What must change in the database of tab `target` to make it the same as the
    /// database of tab `source`. Both tabs must use the same kind of database.
    pub async fn compare_schemas(
        &self,
        source: &str,
        target: &str,
    ) -> AppResult<Vec<SchemaChange>> {
        let (changes, _) = self.schema_changes(source, target).await?;
        Ok(changes.into_iter().map(|change| change.info).collect())
    }

    /// The SQL that makes the changes with an id in `picked`, in a safe order. It only
    /// builds the SQL. Nothing runs. The compare runs again here, so the SQL always
    /// comes from what the databases hold now, never from text the frontend sends.
    pub async fn schema_sync_sql(
        &self,
        source: &str,
        target: &str,
        picked: &[String],
    ) -> AppResult<SyncSql> {
        let (changes, driver) = self.schema_changes(source, target).await?;
        let chosen: Vec<&Change> = changes
            .iter()
            .filter(|change| picked.contains(&change.info.id))
            .collect();
        let from = self.repository(source)?;
        let to = self.repository(target)?;
        let rules = SyncRules::of(driver);
        let mut sql = SyncSql {
            statements: Vec::new(),
            warnings: SchemaCompare::skipped(&chosen, rules),
            database: Self::current_database(to.as_ref()).await?,
        };
        for step in SchemaCompare::plan(&chosen, rules) {
            match Self::step_sql(from.as_ref(), to.as_ref(), &step).await {
                Ok(statements) => sql.statements.extend(statements),
                Err(error) => sql.warnings.push(format!("{}: {error}", step.label())),
            }
        }
        Ok(sql)
    }

    /// Builds the sync SQL again from what the two databases hold now, and runs it on tab
    /// `target` in one transaction, so a failure keeps nothing. No SQL text comes from
    /// the frontend. `reviewed` is the SQL the user saw. When the new SQL is not the same
    /// (a tab changed its database, or someone changed a schema), nothing runs.
    /// `database` is the target database of the reviewed SQL. When the target tab works on
    /// another one now (a typed `USE`), nothing runs either.
    /// `MySQL` commits each `CREATE`, `ALTER` and `DROP` by itself. There a failure keeps
    /// the statements before it.
    pub async fn run_schema_sync(
        &self,
        source: &str,
        target: &str,
        picked: &[String],
        reviewed: &[String],
        database: Option<&str>,
    ) -> AppResult<()> {
        let sql = self.schema_sync_sql(source, target, picked).await?;
        if sql.database.as_deref() != database {
            return Err(OTHER_DATABASE.into());
        }
        if sql.statements != reviewed {
            return Err(SYNC_SQL_CHANGED.into());
        }
        if sql.statements.is_empty() {
            return Err("There is no SQL to run".into());
        }
        let mut script = Vec::with_capacity(sql.statements.len());
        for (index, sql) in sql.statements.into_iter().enumerate() {
            script.push(Statement {
                label: format!("Statement {}", index + 1),
                sql,
                params: Vec::new(),
                data: None,
            });
        }
        self.repository(target)?.run_script(script).await
    }

    /// The database the connection works on now. `None` when that cannot change (`SQLite`).
    pub(super) async fn current_database(repository: &dyn Repository) -> AppResult<Option<String>> {
        let Some(sql) = repository.current_database_sql() else {
            return Ok(None);
        };
        let sets = repository.query(sql).await?;
        Ok(sets
            .into_iter()
            .next()
            .and_then(|set| set.rows.into_iter().next())
            .and_then(|row| row.into_iter().next())
            .flatten())
    }

    /// Fails when the two tabs use different kinds of database. Gives the kind.
    pub(super) fn same_kind(driver: Driver, other: Driver) -> AppResult<Driver> {
        if driver != other {
            return Err(AppError::Message(format!(
                "Both tabs must use the same kind of database. \
                The source is {} and the target is {}.",
                dialect(driver),
                dialect(other)
            )));
        }
        Ok(driver)
    }

    /// The differences of the two tabs, and the kind of database they both use.
    async fn schema_changes(&self, source: &str, target: &str) -> AppResult<(Vec<Change>, Driver)> {
        let from = self.connection(source)?;
        let to = self.connection(target)?;
        let driver = Self::same_kind(from.config.driver, to.config.driver)?;
        let (wanted, found) = tokio::try_join!(
            Self::snapshot(from.repository.as_ref()),
            Self::snapshot(to.repository.as_ref())
        )?;
        Ok((SchemaCompare::diff(&wanted, &found), driver))
    }

    /// The tables of the current database with their columns, indexes and foreign keys.
    /// Views are left out.
    // ponytail: two catalog queries per table. Read the whole catalog in one query per
    // driver if a database with very many tables compares too slowly.
    async fn snapshot(repository: &dyn Repository) -> AppResult<Vec<TableSchema>> {
        let mut tables = Vec::new();
        for table in repository.tables().await? {
            if table.kind == VIEW {
                continue;
            }
            let mut columns = repository.columns(&table).await?;
            repository.default_expressions(&table, &mut columns).await?;
            let keys = repository.table_keys(&table).await?;
            tables.push(TableSchema {
                table,
                columns,
                indexes: keys.indexes,
                foreign_keys: keys.foreign_keys,
            });
        }
        Ok(tables)
    }

    /// The SQL of one step. The statements run on `target`. A new table takes its
    /// `CREATE TABLE` from `source`.
    async fn step_sql(
        source: &dyn Repository,
        target: &dyn Repository,
        step: &Step,
    ) -> AppResult<Vec<String>> {
        Ok(match step {
            Step::DropForeignKey(table, key) => {
                target.check_foreign_key_change()?;
                if key.name.is_empty() {
                    return Err(UNNAMED_FOREIGN_KEY.into());
                }
                vec![
                    target
                        .drop_foreign_key_sql(&target.table_name(table), &target.quote(&key.name)),
                ]
            }
            Step::DropIndex(table, index) => vec![target.drop_any_index_sql(table, index)],
            Step::DropTable(table) => vec![Self::drop_table_sql(target, table)],
            Step::DropColumn(table, name) => vec![target.drop_column_sql(table, name)],
            Step::CreateTable(schema) => {
                vec![Exporter::create_table_sql(source, &schema.table, &schema.columns).await?]
            }
            Step::AddColumn(table, column) => {
                let mut spec = Self::spec(column);
                if let Some(serial) = target.serial_type(column) {
                    serial.clone_into(&mut spec.data_type);
                    spec.default = None;
                }
                let mut definition = Self::column_definition(target, &spec, column.nullable)?;
                if SchemaCompare::numbers_itself(column) {
                    definition.push(' ');
                    definition.push_str(target.auto_increment());
                } else if !SchemaCompare::plain_extra(column) {
                    return Err(COLUMN_RULE_NOT_COPIED.into());
                }
                vec![target.add_column_sql(&target.table_name(table), &definition)]
            }
            Step::AlterColumn(table, old, new) => {
                // A generated column keeps its expression as its default (`PostgreSQL`).
                // `SET DEFAULT` cannot change it. `MySQL` marks a plain expression default
                // like `CURRENT_TIMESTAMP` with `DEFAULT_GENERATED`, which SQL can change.
                let generated = old.extra == "generated" || new.extra == "generated";
                let rule_changed = !old.extra.eq_ignore_ascii_case(&new.extra)
                    && (!SchemaCompare::plain_extra(old) || !SchemaCompare::plain_extra(new));
                if generated || rule_changed {
                    return Err(COLUMN_RULE_CHANGED.into());
                }
                target
                    .modify_column_sql(table, old, &Self::spec(new))
                    .await?
            }
            Step::AddIndex(table, index) => {
                // The database's own statement keeps the kind, order, `INCLUDE` and `WHERE`.
                if let Some(definition) = &index.definition {
                    return Ok(vec![definition.clone()]);
                }
                if index.expression {
                    return Err(EXPRESSION_INDEX.into());
                }
                if index.partial {
                    return Err(PARTIAL_INDEX.into());
                }
                vec![Self::index_sql(target, table, index)]
            }
            Step::AddForeignKey(table, key) => {
                target.check_foreign_key_change()?;
                if key.name.is_empty() {
                    return Err(UNNAMED_FOREIGN_KEY.into());
                }
                vec![Self::foreign_key_sql(target, table, key)]
            }
        })
    }

    /// A column of the source as the column helpers take it.
    fn spec(column: &ColumnInfo) -> ColumnSpec {
        ColumnSpec {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable,
            default: column.default.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str) -> TableRef {
        TableRef {
            schema: None,
            name: name.to_owned(),
            kind: "table".to_owned(),
        }
    }

    fn column(name: &str, data_type: &str, nullable: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.to_owned(),
            data_type: data_type.to_owned(),
            nullable,
            default: None,
            primary_key: false,
            extra: String::new(),
        }
    }

    fn index(name: &str, columns: &[&str], unique: bool) -> IndexInfo {
        IndexInfo {
            name: name.to_owned(),
            columns: columns.iter().map(|column| (*column).to_owned()).collect(),
            unique,
            primary: false,
            expression: false,
            partial: false,
            constraint: false,
            definition: None,
        }
    }

    fn key(name: &str, column: &str, ref_table: &str) -> ForeignKey {
        ForeignKey {
            name: name.to_owned(),
            columns: vec![column.to_owned()],
            ref_schema: None,
            ref_table: ref_table.to_owned(),
            ref_columns: vec!["id".to_owned()],
            on_delete: "NO ACTION".to_owned(),
            on_update: "NO ACTION".to_owned(),
        }
    }

    fn schema(name: &str, columns: Vec<ColumnInfo>) -> TableSchema {
        let mut id = column("id", "int", false);
        id.primary_key = true;
        let mut all = vec![id];
        all.extend(columns);
        TableSchema {
            table: table(name),
            columns: all,
            indexes: vec![IndexInfo {
                name: "PRIMARY".to_owned(),
                columns: vec!["id".to_owned()],
                unique: true,
                primary: true,
                expression: false,
                partial: false,
                constraint: true,
                definition: None,
            }],
            foreign_keys: Vec::new(),
        }
    }

    fn actions(changes: &[Change]) -> Vec<(String, ChangeAction, String)> {
        changes
            .iter()
            .map(|change| {
                (
                    change.info.table.name.clone(),
                    change.info.action,
                    change.info.name.clone(),
                )
            })
            .collect()
    }

    /// Source and target of the tests: `users` changed, `posts` only in the source,
    /// `old` only in the target.
    fn pair() -> (Vec<TableSchema>, Vec<TableSchema>) {
        let mut users = schema(
            "users",
            vec![
                column("name", "varchar(50)", false),
                column("email", "text", true),
            ],
        );
        users.indexes.push(index("users_name", &["name"], true));
        users.indexes.push(index("users_email", &["email"], false));
        let mut posts = schema("posts", vec![column("user_id", "int", false)]);
        posts.indexes.push(index("posts_user", &["user_id"], false));
        posts
            .foreign_keys
            .push(key("posts_user_fk", "user_id", "users"));
        let source = vec![users, posts];

        let mut users = schema(
            "users",
            vec![
                column("name", "varchar(20)", true),
                column("age", "int", true),
            ],
        );
        users
            .indexes
            .push(index("users_name", &["name", "age"], true));
        users.indexes.push(index("users_age", &["age"], false));
        users
            .indexes
            .push(index("sqlite_autoindex_users_1", &["name"], true));
        let mut old = schema("old", vec![column("user_id", "int", true)]);
        old.foreign_keys
            .push(key("old_user_fk", "user_id", "users"));
        let target = vec![users, old];
        (source, target)
    }

    #[test]
    fn finds_every_kind_of_difference() {
        let (source, target) = pair();
        let changes = SchemaCompare::diff(&source, &target);
        assert_eq!(
            actions(&changes),
            vec![
                ("old".to_owned(), ChangeAction::DropTable, String::new()),
                ("posts".to_owned(), ChangeAction::CreateTable, String::new()),
                (
                    "users".to_owned(),
                    ChangeAction::AlterColumn,
                    "name".to_owned()
                ),
                (
                    "users".to_owned(),
                    ChangeAction::AddColumn,
                    "email".to_owned()
                ),
                (
                    "users".to_owned(),
                    ChangeAction::DropColumn,
                    "age".to_owned()
                ),
                (
                    "users".to_owned(),
                    ChangeAction::AlterIndex,
                    "users_name".to_owned()
                ),
                (
                    "users".to_owned(),
                    ChangeAction::AddIndex,
                    "users_email".to_owned()
                ),
                (
                    "users".to_owned(),
                    ChangeAction::DropIndex,
                    "users_age".to_owned()
                ),
            ]
        );
        assert_eq!(changes[2].info.detail, "varchar(20) → varchar(50) NOT NULL");
        assert_eq!(changes[5].info.detail, "UNIQUE (name, age) → UNIQUE (name)");
        assert_eq!(changes[1].info.detail, "2 columns");
        // Every id is different, so each difference can be picked alone.
        let mut ids: Vec<&str> = changes
            .iter()
            .map(|change| change.info.id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), changes.len());
    }

    #[test]
    fn same_schemas_have_no_differences() {
        let (source, _) = pair();
        assert!(SchemaCompare::diff(&source, &source).is_empty());
        // The case of a type does not count.
        let mut upper = source.clone();
        upper[0].columns[1].data_type = "VARCHAR(50)".to_owned();
        assert!(SchemaCompare::diff(&source, &upper).is_empty());
    }

    #[test]
    fn foreign_keys_match_by_what_they_do() {
        let mut source = schema("posts", vec![column("user_id", "int", false)]);
        source.foreign_keys.push(key("", "user_id", "users"));
        let mut target = source.clone();
        target.foreign_keys[0].name = "other_name".to_owned();
        assert!(SchemaCompare::diff(&[source.clone()], &[target.clone()]).is_empty());

        target.foreign_keys[0].on_delete = "CASCADE".to_owned();
        let changes = SchemaCompare::diff(&[source], &[target]);
        let found: Vec<ChangeAction> = changes.iter().map(|change| change.info.action).collect();
        assert_eq!(
            found,
            vec![ChangeAction::AddForeignKey, ChangeAction::DropForeignKey]
        );
        assert_eq!(
            changes[1].info.detail,
            "(user_id) → users (id) ON DELETE CASCADE"
        );
        assert_ne!(changes[0].info.id, changes[1].info.id);
    }

    fn phases(steps: &[Step]) -> Vec<String> {
        steps.iter().map(Step::label).collect()
    }

    #[test]
    fn plan_runs_in_a_safe_order() {
        let (source, target) = pair();
        let changes = SchemaCompare::diff(&source, &target);
        let all: Vec<&Change> = changes.iter().collect();
        let steps = SchemaCompare::plan(&all, SyncRules::of(Driver::Postgres));
        assert_eq!(
            phases(&steps),
            vec![
                "Drop foreign key (user_id) → users (id) of old",
                "Drop index users_name of users",
                "Drop index users_age of users",
                "Drop table old",
                "Drop column age of users",
                "Create table posts",
                "Add column email to users",
                "Change column name of users",
                "Add index users_name to users",
                "Add index users_email to users",
                "Add index posts_user to posts",
                "Add foreign key (user_id) → users (id) to posts",
            ]
        );
        let mut last = 0;
        for step in &steps {
            assert!(step.phase() >= last, "{}", step.label());
            last = step.phase();
        }
    }

    #[test]
    fn plan_keeps_what_the_create_holds_and_only_what_is_picked() {
        let (source, target) = pair();
        let changes = SchemaCompare::diff(&source, &target);
        let picked: Vec<&Change> = changes
            .iter()
            .filter(|change| {
                matches!(
                    change.info.action,
                    ChangeAction::CreateTable | ChangeAction::DropTable
                )
            })
            .collect();
        // MySQL: the CREATE has the index and the foreign key inside.
        let mysql = SchemaCompare::plan(&picked, SyncRules::of(Driver::Mysql));
        assert_eq!(
            phases(&mysql),
            vec![
                "Drop foreign key (user_id) → users (id) of old",
                "Drop table old",
                "Create table posts",
            ]
        );
        // SQLite: no foreign key changes, and the index comes after the CREATE.
        let sqlite = SchemaCompare::plan(&picked, SyncRules::of(Driver::Sqlite));
        assert_eq!(
            phases(&sqlite),
            vec![
                "Drop table old",
                "Create table posts",
                "Add index posts_user to posts",
            ]
        );
    }

    #[test]
    fn an_index_that_cannot_be_made_keeps_the_old_one() {
        let mut source = schema("users", vec![column("email", "text", true)]);
        let mut target = source.clone();
        source
            .indexes
            .push(index("users_email", &["lower(email)"], false));
        source.indexes[1].expression = true;
        target.indexes.push(index("users_email", &["email"], false));
        let changes = SchemaCompare::diff(&[source], &[target]);
        let all: Vec<&Change> = changes.iter().collect();
        let steps = SchemaCompare::plan(&all, SyncRules::of(Driver::Postgres));
        assert_eq!(phases(&steps), vec!["Add index users_email to users"]);
    }

    #[test]
    fn a_changed_auto_increment_is_a_difference() {
        let source = schema("users", vec![column("n", "int", false)]);
        let mut target = source.clone();
        target.columns[1].extra = "auto_increment".to_owned();
        let changes = SchemaCompare::diff(&[source], &[target]);
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].info.detail,
            "int NOT NULL auto_increment → int NOT NULL"
        );
    }

    #[test]
    fn a_new_table_comes_after_the_new_table_it_points_at() {
        let mut child = schema("a_child", vec![column("parent_id", "int", false)]);
        child
            .foreign_keys
            .push(key("child_parent", "parent_id", "z_parent"));
        let parent = schema("z_parent", Vec::new());
        let changes = SchemaCompare::diff(&[child, parent], &[]);
        let all: Vec<&Change> = changes.iter().collect();
        let steps = SchemaCompare::plan(&all, SyncRules::of(Driver::Mysql));
        assert_eq!(
            phases(&steps),
            vec!["Create table z_parent", "Create table a_child"]
        );
    }

    #[test]
    fn a_new_table_with_a_generated_column_is_only_a_warning_on_postgres() {
        let mut total = column("total", "int", true);
        total.extra = "generated".to_owned();
        let mut source = schema("orders", vec![total]);
        source
            .indexes
            .push(index("orders_total", &["total"], false));
        let changes = SchemaCompare::diff(&[source], &[]);
        let all: Vec<&Change> = changes.iter().collect();
        let postgres = SyncRules::of(Driver::Postgres);
        assert!(SchemaCompare::plan(&all, postgres).is_empty());
        assert_eq!(
            SchemaCompare::skipped(&all, postgres),
            vec![format!("Create table orders: {TABLE_NOT_COPIED}")]
        );
        // MySQL copies its own `CREATE TABLE`, which keeps the generated column.
        let mysql = SyncRules::of(Driver::Mysql);
        assert_eq!(
            phases(&SchemaCompare::plan(&all, mysql)),
            vec!["Create table orders"]
        );
        assert_eq!(SchemaCompare::skipped(&all, mysql), Vec::<String>::new());
    }
}
