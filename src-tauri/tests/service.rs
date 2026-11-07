//! One end-to-end check of the service against real databases.
//!
//! `SQLite` always runs. `MySQL`, `PostgreSQL` and SQL Server run only when these are set:
//! `SQLPRO_TEST_MYSQL=host:port:user:password:database`
//! `SQLPRO_TEST_MARIADB=host:port:user:password:database` (`MariaDB` through the `MySQL` driver)
//! `SQLPRO_TEST_POSTGRES=host:port:user:password:database`
//! `SQLPRO_TEST_MSSQL=host:port:user:password:database` (the database is created when missing)
//! Add `SQLPRO_TEST_SSH=host:port:user:password` to run them through an SSH tunnel.

use sql_pro_lib::model::DbObject;
use sql_pro_lib::model::{
    AccessLevel, BrowseRequest, ColumnSpec, ConnectionConfig, CsvColumn, CsvImport, DataKinds,
    DbUser, Diagram, Driver, ExportFormat, ExportRequest, FilterOperator, FilterRule, ForeignKey,
    IndexInfo, NewColumn, NewForeignKey, NewIndex, NewUser, PlanMeasure, ReferentialAction,
    Relation, RowUpdate, RowsExport, RowsSql, SaveResult, SshConfig, TableRef, Values,
};
use std::time::{Duration, Instant};

use sql_pro_lib::repository::RepositoryFactory;
use sql_pro_lib::service::ConnectionService;

const ID: &str = "test";
const TABLE: &str = "sqlpro_check";
const EXTRA: &str = "sqlpro_extra";
const EXTRA_VIEW: &str = "sqlpro_extra_view";
const MADE: &str = "sqlpro_made";
/// The copy of the test table that the import makes.
const IMPORTED: &str = "sqlpro_imported";
/// The database that `restore` imports a `pg_dump` backup into.
const RESTORED: &str = "sqlpro_restored";
/// A name that must be quoted to work.
const RENAMED: &str = "sqlpro renamed \"x\"";

/// The connection id of the copy made by `clone_connection`.
const CLONE_ID: &str = "test-clone";

/// The connection id of the test account.
const USER_ID: &str = "test-user";
const USER: &str = "sqlpro_test_user";
const PASSWORD: &str = "first-Pass-1";
/// A name with every character that could break SQL. It is 32 long, the `MySQL` limit.
const TRICKY_USER: &str = r#"sp'"\`;DROP TABLE sqlpro_check--"#;
const TRICKY_PASSWORD: &str = r#"p'a"s\s`; DROP TABLE sqlpro_check; -- "#;

fn values(pairs: &[(&str, Option<&str>)]) -> Values {
    let mut values = Values::new();
    for (name, value) in pairs {
        values.insert((*name).to_owned(), value.map(str::to_owned));
    }
    values
}

#[allow(clippy::unnecessary_wraps, reason = "cells are Option<String>")]
fn text(value: &str) -> Option<String> {
    Some(value.to_owned())
}

/// Set `SQLPRO_TEST_SSH=host:port:user:password` to run the servers through a tunnel.
fn ssh_from_env() -> Option<SshConfig> {
    let raw = std::env::var("SQLPRO_TEST_SSH").ok()?;
    let parts: Vec<&str> = raw.splitn(4, ':').collect();
    let [host, port, user, password] = parts[..] else {
        panic!("SQLPRO_TEST_SSH must be host:port:user:password");
    };
    Some(SshConfig {
        host: host.to_owned(),
        port: port.parse().expect("ssh port is a number"),
        user: user.to_owned(),
        password: password.to_owned(),
        key_path: String::new(),
    })
}

/// A temp file of this test run. The name of the test thread is in it, because the
/// tests run at the same time and `MySQL` and `MariaDB` both use `Driver::Mysql`.
fn temp_path(kind: &str, extension: &str) -> std::path::PathBuf {
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("main");
    let name = format!("sqlpro-{kind}-{}-{test}.{extension}", std::process::id());
    std::env::temp_dir().join(name)
}

fn config_from_env(name: &str, driver: Driver) -> Option<ConnectionConfig> {
    let raw = std::env::var(name).ok()?;
    let parts: Vec<&str> = raw.splitn(5, ':').collect();
    let [host, port, user, password, database] = parts[..] else {
        panic!("{name} must be host:port:user:password:database");
    };
    Some(ConnectionConfig {
        driver,
        host: host.to_owned(),
        port: port.parse().expect("port is a number"),
        user: user.to_owned(),
        password: password.to_owned(),
        database: database.to_owned(),
        ssh: ssh_from_env(),
        ssl: sql_pro_lib::model::SslConfig::default(),
    })
}

fn request(table: &TableRef, rules: &[Rule], filter: &str) -> BrowseRequest {
    let mut filters = Vec::new();
    for (column, operator, value) in rules {
        filters.push(FilterRule {
            column: (*column).to_owned(),
            operator: *operator,
            value: (*value).to_owned(),
        });
    }
    BrowseRequest {
        table: table.clone(),
        filter: filter.to_owned(),
        filters,
        order_by: String::new(),
        descending: false,
        limit: 100,
        offset: 0,
        total: None,
    }
}

/// Moving to another page sends the known total, and the count is skipped.
/// Expects rows 1 and 2.
async fn paging(service: &ConnectionService, table: &TableRef) {
    let mut next = request(table, &[], "");
    next.limit = 1;
    next.offset = 1;
    next.total = Some(99);
    let loaded = service.browse(ID, &next).await.expect("next page");
    assert_eq!(loaded.total, 99, "the known total comes back");
    assert_eq!(loaded.rows.len(), 1);
    assert_eq!(loaded.rows[0][0], text("2"));

    // No rows still gives a count of 0, not a number read from the rows.
    let empty = service
        .browse(ID, &request(table, &[], "id > 100"))
        .await
        .expect("empty");
    assert_eq!((empty.rows.len(), empty.total), (0, 0));

    // A `--` comment in the filter must not hide the count.
    let commented = service
        .browse(ID, &request(table, &[], "id = 2 -- note"))
        .await
        .expect("comment");
    assert_eq!((commented.rows.len(), commented.total), (1, 1));
}

async fn page(service: &ConnectionService, table: &TableRef) -> sql_pro_lib::model::TablePage {
    let request = request(table, &[], "");
    service.browse(ID, &request).await.expect("browse")
}

/// One filter rule: column, operator, value.
type Rule = (&'static str, FilterOperator, &'static str);

/// The ids of the rows that match. Also checks that the row count agrees.
async fn matching(
    service: &ConnectionService,
    table: &TableRef,
    rules: &[Rule],
    filter: &str,
) -> Vec<String> {
    let request = request(table, rules, filter);
    let loaded = service.browse(ID, &request).await.expect("filter");
    let mut ids = Vec::new();
    for row in &loaded.rows {
        ids.push(row[0].clone().expect("id"));
    }
    assert_eq!(loaded.total, ids.len() as u64, "{rules:?}");
    ids
}

/// Filter rules. Expects row 1 (name O'Brien) and row 2 (name NULL), and leaves them as they were.
async fn filters(service: &ConnectionService, table: &TableRef) {
    use FilterOperator::{
        Contains, EndsWith, Eq, Gt, In, IsNotNull, IsNull, NotContains, StartsWith,
    };

    // A name with every character that could break a text value or a LIKE pattern.
    let tricky = values(&[("id", Some("3")), ("name", Some(r"a'b%c\d_!"))]);
    service
        .insert_row(ID, table, tricky)
        .await
        .expect("insert 3");

    let cases: &[(&[Rule], &str, &[&str])] = &[
        (&[("id", Eq, "2")], "", &["2"]),
        (&[("name", Eq, r"a'b%c\d_!")], "", &["3"]),
        (&[("name", Contains, r"'b%c\")], "", &["3"]),
        // `%`, `_` and `!` are plain text, not wildcards.
        (&[("name", Contains, "%")], "", &["3"]),
        (&[("name", Contains, "_!")], "", &["3"]),
        (&[("name", Contains, "O_B")], "", &[]),
        (&[("name", NotContains, "%")], "", &["1"]),
        (&[("name", StartsWith, "a'")], "", &["3"]),
        (&[("name", EndsWith, r"\d_!")], "", &["3"]),
        // LIKE on a number column.
        (&[("id", Contains, "3")], "", &["3"]),
        (&[("id", In, " 1, 3 ,")], "", &["1", "3"]),
        (&[("name", IsNull, "ignored")], "", &["2"]),
        (&[("name", IsNotNull, ""), ("id", Gt, "1")], "", &["3"]),
        // The raw filter is wrapped, so its OR cannot escape the rule.
        (&[("id", Eq, "1")], "id = 2 OR id = 3", &[]),
        (&[("id", In, "1,3")], "id > 1", &["3"]),
        (&[], "id > 1", &["2", "3"]),
        // A value must never break out of its quotes.
        (&[("name", Eq, "x' OR '1'='1")], "", &[]),
        (&[("name", Eq, r"\' OR 1=1 -- ")], "", &[]),
    ];
    for (rules, filter, expected) in cases {
        let found = matching(service, table, rules, filter).await;
        assert_eq!(&found, expected, "{rules:?} {filter}");
    }

    for rules in [[("nope", Eq, "1")], [("id", In, " , ")]] {
        let request = request(table, &rules, "");
        assert!(service.browse(ID, &request).await.is_err(), "{rules:?}");
    }

    delete_row(service, table, "3").await;
}

fn spec(name: &str, data_type: &str, nullable: bool, default: Option<&str>) -> ColumnSpec {
    ColumnSpec {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable,
        default: default.map(str::to_owned),
    }
}

/// Adds, renames, changes and drops a column. The one data row must survive.
async fn schema_changes(service: &ConnectionService, table: &TableRef, driver: Driver) {
    let added = spec("score", "INTEGER", false, Some("7"));
    service
        .add_column(ID, table, &added)
        .await
        .expect("add column");
    let loaded = page(service, table).await;
    assert_eq!(loaded.columns.len(), 5);
    assert_eq!(loaded.columns[4].name, "score");
    assert!(!loaded.columns[4].nullable);
    assert_eq!(loaded.rows[0][4], text("7"));

    // Rename only. This works on all three.
    let current = &loaded.columns[4].data_type;
    let renamed = spec(
        "points",
        current,
        false,
        loaded.columns[4].default.as_deref(),
    );
    service
        .alter_column(ID, table, "score", &renamed)
        .await
        .expect("rename column");
    assert_eq!(page(service, table).await.columns[4].name, "points");

    let loaded = page(service, table).await;
    let changed = spec("points", "VARCHAR(30)", true, Some("'none'"));
    let result = service.alter_column(ID, table, "points", &changed).await;
    if driver == Driver::Sqlite {
        // SQLite cannot do this. It must say so and change nothing.
        assert!(result.is_err());
    } else {
        result.expect("change type, null rule and default");
        let after = page(service, table).await;
        assert!(after.columns[4].nullable);
        assert!(after.columns[4].data_type.to_lowercase().contains("var"));
        assert!(
            after.columns[4]
                .default
                .as_deref()
                .unwrap_or_default()
                .contains("none")
        );
        assert_eq!(after.rows[0][4], text("7"));
        // The other columns are untouched.
        assert_eq!(after.columns[1].data_type, loaded.columns[1].data_type);
    }

    assert!(
        service
            .add_column(ID, table, &spec("", "INTEGER", true, None))
            .await
            .is_err()
    );
    service
        .drop_column(ID, table, "points")
        .await
        .expect("drop column");
    assert_eq!(page(service, table).await.columns.len(), 4);
    assert!(service.drop_column(ID, table, "points").await.is_err());
}

fn names(list: &[&str]) -> Vec<String> {
    let mut names = Vec::with_capacity(list.len());
    for name in list {
        names.push((*name).to_owned());
    }
    names
}

fn new_index(name: &str, columns: &[&str], unique: bool) -> NewIndex {
    NewIndex {
        name: name.to_owned(),
        columns: names(columns),
        unique,
    }
}

async fn index_named(
    service: &ConnectionService,
    table: &TableRef,
    name: &str,
) -> Option<IndexInfo> {
    let keys = service.table_keys(ID, table).await.expect("table keys");
    keys.indexes.into_iter().find(|index| index.name == name)
}

/// Adds and drops indexes of the child table of `keys`.
async fn index_changes(service: &ConnectionService, child: &TableRef) {
    // A name that must be quoted, and two columns in an order the table does not have.
    let odd = "sqlpro k \"idx\"";
    service
        .add_index(ID, child, &new_index(odd, &["note", "code"], true))
        .await
        .expect("add index");
    let added = index_named(service, child, odd).await.expect("new index");
    assert_eq!(added.columns, ["note", "code"]);
    assert!(added.unique && !added.primary);
    service
        .add_index(ID, child, &new_index("sqlpro_k_plain", &["code"], false))
        .await
        .expect("add plain index");
    let plain = index_named(service, child, "sqlpro_k_plain").await;
    assert!(!plain.expect("plain index").unique);

    // Bad input is refused before any SQL runs.
    for bad in [
        new_index(" ", &["code"], false),
        new_index("sqlpro_k_none", &[], false),
        new_index("sqlpro_k_bad", &["nope"], false),
    ] {
        assert!(service.add_index(ID, child, &bad).await.is_err());
    }

    service
        .drop_index(ID, child, odd)
        .await
        .expect("drop index");
    assert!(index_named(service, child, odd).await.is_none());
    assert!(service.drop_index(ID, child, odd).await.is_err());
}

/// Lists, adds and drops indexes and foreign keys of a child table.
async fn keys(service: &ConnectionService, driver: Driver) {
    let setup = "DROP TABLE IF EXISTS sqlpro_k_child; DROP TABLE IF EXISTS sqlpro_k_parent; \
        CREATE TABLE sqlpro_k_parent (id INTEGER PRIMARY KEY, code INTEGER NOT NULL UNIQUE); \
        CREATE TABLE sqlpro_k_child (id INTEGER PRIMARY KEY, parent_id INTEGER, code INTEGER, \
            note VARCHAR(20), \
            CONSTRAINT sqlpro_k_first FOREIGN KEY (parent_id) REFERENCES sqlpro_k_parent (id) \
                ON DELETE CASCADE)";
    service.run_query(ID, setup).await.expect("key tables");
    let child = listed(service, "sqlpro_k_child").await.expect("child");
    let parent = listed(service, "sqlpro_k_parent").await.expect("parent");

    let loaded = service.table_keys(ID, &child).await.expect("table keys");
    let primary = &loaded.indexes[0];
    assert!(
        primary.primary && primary.unique,
        "the primary key comes first"
    );
    assert_eq!(primary.columns, ["id"]);
    let first = &loaded.foreign_keys[0];
    assert_eq!(loaded.foreign_keys.len(), 1);
    assert_eq!(first.columns, ["parent_id"]);
    assert_eq!(
        (first.ref_table.as_str(), first.ref_schema.as_deref()),
        ("sqlpro_k_parent", None)
    );
    assert_eq!(first.ref_columns, ["id"]);
    assert_eq!(first.on_delete, "CASCADE");
    if driver != Driver::Sqlite {
        assert_eq!(first.name, "sqlpro_k_first");
    }
    column_details(service, &child, driver).await;

    index_changes(service, &child).await;
    // The primary key is never dropped as an index.
    let primary = primary.name.clone();
    assert!(service.drop_index(ID, &child, &primary).await.is_err());

    // SQL Server refuses a second cascade path from the same parent.
    let (on_delete, delete_rule) = if driver == Driver::Mssql {
        (ReferentialAction::NoAction, "NO ACTION")
    } else {
        (ReferentialAction::SetNull, "SET NULL")
    };
    let key = NewForeignKey {
        name: "sqlpro_k_second".to_owned(),
        columns: names(&["code"]),
        ref_table: parent.clone(),
        ref_columns: names(&["code"]),
        on_delete,
        on_update: ReferentialAction::Cascade,
    };
    let result = service.add_foreign_key(ID, &child, &key).await;
    if driver == Driver::Sqlite {
        // SQLite cannot do this. It must say so and change nothing.
        assert!(result.is_err());
        assert!(service.drop_foreign_key(ID, &child, "").await.is_err());
        let after = service.table_keys(ID, &child).await.expect("table keys");
        assert_eq!(after.foreign_keys.len(), 1);
    } else {
        result.expect("add foreign key");
        let after = service.table_keys(ID, &child).await.expect("table keys");
        let second: Vec<&ForeignKey> = after
            .foreign_keys
            .iter()
            .filter(|key| key.name == "sqlpro_k_second")
            .collect();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].ref_columns, ["code"]);
        assert_eq!(
            (second[0].on_delete.as_str(), second[0].on_update.as_str()),
            (delete_rule, "CASCADE")
        );

        let mut wrong = NewForeignKey {
            ref_columns: Vec::new(),
            ..key
        };
        assert!(service.add_foreign_key(ID, &child, &wrong).await.is_err());
        wrong.ref_columns = names(&["nope"]);
        assert!(service.add_foreign_key(ID, &child, &wrong).await.is_err());

        service
            .drop_foreign_key(ID, &child, "sqlpro_k_first")
            .await
            .expect("drop foreign key");
        let after = service.table_keys(ID, &child).await.expect("table keys");
        assert_eq!(after.foreign_keys.len(), 1);
        assert_eq!(after.foreign_keys[0].name, "sqlpro_k_second");
        assert!(
            service
                .drop_foreign_key(ID, &child, "sqlpro_k_first")
                .await
                .is_err()
        );
    }

    service
        .run_query(ID, "DROP TABLE sqlpro_k_child; DROP TABLE sqlpro_k_parent")
        .await
        .expect("drop key tables");
}

/// The character set, collation and comment of the columns, for the Structure view.
async fn column_details(service: &ConnectionService, child: &TableRef, driver: Driver) {
    let comment = match driver {
        Driver::Mysql => {
            "ALTER TABLE sqlpro_k_child MODIFY note VARCHAR(20) COMMENT 'the note'".to_owned()
        }
        Driver::Postgres => "COMMENT ON COLUMN sqlpro_k_child.note IS 'the note'".to_owned(),
        Driver::Mssql => format!(
            "EXEC sp_addextendedproperty 'MS_Description', 'the note', 'SCHEMA', '{}', \
                'TABLE', 'sqlpro_k_child', 'COLUMN', 'note'",
            child.schema.as_deref().unwrap_or("dbo")
        ),
        Driver::Sqlite => String::new(),
    };
    if !comment.is_empty() {
        service
            .run_query(ID, &comment)
            .await
            .expect("column comment");
    }
    let details = service
        .table_keys(ID, child)
        .await
        .expect("table keys")
        .columns;
    if driver == Driver::Sqlite {
        // SQLite keeps none of these.
        assert_eq!(details, []);
        return;
    }
    let names: Vec<&str> = details.iter().map(|detail| detail.name.as_str()).collect();
    assert_eq!(names, ["id", "parent_id", "code", "note"]);
    let (id, note) = (&details[0], &details[3]);
    assert_eq!(
        (
            id.charset.as_deref(),
            id.collation.as_deref(),
            id.comment.as_deref()
        ),
        (None, None, None)
    );
    assert_eq!(note.comment.as_deref(), Some("the note"));
    assert!(
        note.collation.is_some(),
        "a text column has a collation: {note:?}"
    );
    // Only MySQL keeps a character set per column.
    assert_eq!(note.charset.is_some(), driver == Driver::Mysql, "{note:?}");
}

/// `table_keys` sends indexes, foreign keys and column details as one batch.
/// Each part must land in its own place, also when a part has no rows.
async fn keys_together(service: &ConnectionService, driver: Driver) {
    let setup = "DROP TABLE IF EXISTS sqlpro_t_child; DROP TABLE IF EXISTS sqlpro_t_parent; \
        DROP TABLE IF EXISTS sqlpro_t_bare; \
        CREATE TABLE sqlpro_t_parent (a INTEGER NOT NULL, b INTEGER NOT NULL, PRIMARY KEY (a, b)); \
        CREATE TABLE sqlpro_t_child (id INTEGER PRIMARY KEY, pa INTEGER, pb INTEGER, \
            note VARCHAR(20), \
            CONSTRAINT sqlpro_t_fk FOREIGN KEY (pa, pb) REFERENCES sqlpro_t_parent (a, b)); \
        CREATE TABLE sqlpro_t_bare (x INTEGER, y VARCHAR(10))";
    service
        .run_query(ID, setup)
        .await
        .expect("batch key tables");
    let sqlite = driver == Driver::Sqlite;
    let detail_names = |keys: &sql_pro_lib::model::TableKeys| -> Vec<String> {
        keys.columns
            .iter()
            .map(|detail| detail.name.clone())
            .collect()
    };

    // A composite foreign key, and the column details after it.
    let child = listed(service, "sqlpro_t_child").await.expect("child");
    let keys = service.table_keys(ID, &child).await.expect("child keys");
    assert!(keys.indexes[0].primary);
    assert_eq!(keys.indexes[0].columns, ["id"]);
    assert_eq!(keys.foreign_keys.len(), 1);
    let key = &keys.foreign_keys[0];
    assert_eq!(key.columns, ["pa", "pb"]);
    assert_eq!(key.ref_table, "sqlpro_t_parent");
    assert_eq!(key.ref_columns, ["a", "b"]);
    if sqlite {
        assert_eq!(keys.columns, []);
    } else {
        assert_eq!(detail_names(&keys), ["id", "pa", "pb", "note"]);
        assert!(keys.columns[3].collation.is_some());
    }

    // No foreign key: the empty middle set must not shift the details.
    let parent = listed(service, "sqlpro_t_parent").await.expect("parent");
    let keys = service.table_keys(ID, &parent).await.expect("parent keys");
    assert_eq!(keys.indexes.len(), 1);
    assert_eq!(keys.indexes[0].columns, ["a", "b"]);
    assert_eq!(keys.foreign_keys, []);
    if !sqlite {
        assert_eq!(detail_names(&keys), ["a", "b"]);
    }

    // No index and no foreign key: only the details have rows.
    let bare = listed(service, "sqlpro_t_bare").await.expect("bare");
    let keys = service.table_keys(ID, &bare).await.expect("bare keys");
    assert_eq!(keys.indexes, []);
    assert_eq!(keys.foreign_keys, []);
    if !sqlite {
        assert_eq!(detail_names(&keys), ["x", "y"]);
    }

    service
        .run_query(
            ID,
            "DROP TABLE sqlpro_t_child; DROP TABLE sqlpro_t_parent; DROP TABLE sqlpro_t_bare",
        )
        .await
        .expect("drop batch key tables");
}

async fn listed(service: &ConnectionService, name: &str) -> Option<TableRef> {
    let tables = service.tables(ID).await.expect("tables");
    tables.into_iter().find(|table| table.name == name)
}

/// Truncates and drops an extra table, and a view on it.
async fn drop_and_truncate(service: &ConnectionService) {
    let setup = format!(
        "DROP VIEW IF EXISTS {EXTRA_VIEW}; DROP TABLE IF EXISTS {EXTRA}; \
        CREATE TABLE {EXTRA} (id INTEGER PRIMARY KEY); \
        INSERT INTO {EXTRA} (id) VALUES (1); INSERT INTO {EXTRA} (id) VALUES (2)"
    );
    service.run_query(ID, &setup).await.expect("extra table");
    // SQL Server wants a CREATE VIEW alone in its batch.
    let view_sql = format!("CREATE VIEW {EXTRA_VIEW} AS SELECT id FROM {EXTRA}");
    service.run_query(ID, &view_sql).await.expect("extra view");
    let table = listed(service, EXTRA).await.expect("extra is listed");
    let view = listed(service, EXTRA_VIEW).await.expect("view is listed");
    assert_eq!(view.kind, "view");
    assert_eq!(page(service, &table).await.total, 2);

    // A view cannot be truncated. Its table keeps the rows.
    assert!(service.truncate_table(ID, &view).await.is_err());
    // The kind comes from the database. A view sent as a table is still a view.
    let fake = TableRef {
        kind: "table".to_owned(),
        ..view.clone()
    };
    assert!(service.truncate_table(ID, &fake).await.is_err());
    assert_eq!(page(service, &table).await.total, 2);

    service.truncate_table(ID, &table).await.expect("truncate");
    assert_eq!(page(service, &table).await.total, 0);
    assert!(listed(service, EXTRA).await.is_some());

    // A name that is not a table never reaches the SQL.
    let mut unknown = table.clone();
    unknown.name = format!("{TABLE}; DROP TABLE {TABLE}");
    assert!(service.drop_table(ID, &unknown).await.is_err());
    assert!(service.truncate_table(ID, &unknown).await.is_err());

    service.drop_table(ID, &fake).await.expect("drop view");
    assert!(listed(service, EXTRA_VIEW).await.is_none());
    assert!(listed(service, EXTRA).await.is_some());
    service.drop_table(ID, &table).await.expect("drop table");
    assert!(listed(service, EXTRA).await.is_none());
    assert!(service.drop_table(ID, &table).await.is_err());
    assert!(listed(service, TABLE).await.is_some());
}

const TRIGGER: &str = "sqlpro_trg";
const ROUTINE: &str = "sqlpro_func";
const PROCEDURE: &str = "sqlpro_proc";
const OBJECT_VIEW: &str = "sqlpro_obj_view";

async fn object(service: &ConnectionService, kind: &str, name: &str) -> Vec<DbObject> {
    let objects = service.objects(ID).await.expect("objects");
    objects
        .into_iter()
        .filter(|object| object.kind == kind && object.name == name)
        .collect()
}

/// Shows and drops a procedure and functions. `PostgreSQL` has two functions with one name.
async fn routines(service: &ConnectionService, driver: Driver) {
    let procedure = object(service, "procedure", PROCEDURE).await;
    assert_eq!(procedure.len(), 1);
    let sql = service
        .object_definition(ID, &procedure[0])
        .await
        .expect("procedure sql");
    assert!(sql.to_uppercase().contains("PROCEDURE"), "{sql}");
    // A procedure sent as a function is not known.
    let mut fake = procedure[0].clone();
    "function".clone_into(&mut fake.kind);
    assert!(service.drop_object(ID, &fake).await.is_err());
    service
        .drop_object(ID, &procedure[0])
        .await
        .expect("drop procedure");
    assert_eq!(object(service, "procedure", PROCEDURE).await.len(), 0);

    let functions = object(service, "function", ROUTINE).await;
    let count = if driver == Driver::Postgres { 2 } else { 1 };
    assert_eq!(functions.len(), count);
    let sql = service
        .object_definition(ID, &functions[0])
        .await
        .expect("function sql");
    assert!(sql.to_uppercase().contains("FUNCTION"), "{sql}");
    // The argument types come from the catalog, never from the caller.
    let mut fake = functions[0].clone();
    fake.args = Some(format!("integer); DROP TABLE {TABLE}; --"));
    assert!(service.drop_object(ID, &fake).await.is_err());
    // Two functions with one name: the drop takes only the one with these types.
    service
        .drop_object(ID, &functions[0])
        .await
        .expect("drop function");
    assert_eq!(object(service, "function", ROUTINE).await.len(), count - 1);
    for left in object(service, "function", ROUTINE).await {
        service.drop_object(ID, &left).await.expect("drop other");
    }
}

/// Lists, shows and drops a trigger, a procedure and functions. Shows the `CREATE VIEW` of a view.
async fn objects(service: &ConnectionService, driver: Driver) {
    let setup = match driver {
        Driver::Sqlite => {
            format!("CREATE TRIGGER {TRIGGER} AFTER INSERT ON {TABLE} BEGIN SELECT 1; END")
        }
        // Each CREATE must be alone in its batch, so they run one by one below.
        Driver::Mssql => [
            format!("CREATE TRIGGER {TRIGGER} ON {TABLE} AFTER INSERT AS SET NOCOUNT ON"),
            format!("CREATE PROCEDURE {PROCEDURE} AS SELECT 1"),
            format!("CREATE FUNCTION {ROUTINE}(@a INT) RETURNS INT AS BEGIN RETURN @a + 1 END"),
        ]
        .join("\n;;\n"),
        Driver::Mysql => format!(
            "CREATE TRIGGER {TRIGGER} BEFORE INSERT ON {TABLE} FOR EACH ROW SET NEW.note = NEW.note; \
            CREATE PROCEDURE {PROCEDURE}() SELECT 1; \
            CREATE FUNCTION {ROUTINE}(a INT) RETURNS INT DETERMINISTIC RETURN a + 1"
        ),
        Driver::Postgres => format!(
            "CREATE FUNCTION {TRIGGER}_fn() RETURNS trigger LANGUAGE plpgsql \
                AS $$ BEGIN RETURN NEW; END $$; \
            CREATE TRIGGER {TRIGGER} BEFORE INSERT ON {TABLE} \
                FOR EACH ROW EXECUTE FUNCTION {TRIGGER}_fn(); \
            CREATE PROCEDURE {PROCEDURE}() LANGUAGE sql AS 'SELECT 1'; \
            CREATE FUNCTION {ROUTINE}(a integer) RETURNS integer LANGUAGE sql AS 'SELECT a + 1'; \
            CREATE FUNCTION {ROUTINE}(a text) RETURNS text LANGUAGE sql AS 'SELECT a'"
        ),
    };
    let view_sql = format!("CREATE VIEW {OBJECT_VIEW} AS SELECT id FROM {TABLE}");
    for statement in setup.split("\n;;\n") {
        service
            .run_query(ID, statement)
            .await
            .expect("objects setup");
    }
    service.run_query(ID, &view_sql).await.expect("view setup");

    // A view shows its CREATE statement.
    let view = listed(service, OBJECT_VIEW).await.expect("view is listed");
    let view_object = DbObject {
        kind: "view".to_owned(),
        schema: view.schema.clone(),
        name: view.name.clone(),
        table: None,
        args: None,
    };
    let sql = service
        .object_definition(ID, &view_object)
        .await
        .expect("view sql");
    assert!(sql.to_uppercase().contains("VIEW"), "{sql}");
    assert!(sql.contains(TABLE), "{sql}");
    service.drop_table(ID, &view).await.expect("drop view");

    let trigger = object(service, "trigger", TRIGGER).await;
    assert_eq!(trigger.len(), 1);
    assert_eq!(trigger[0].table.as_deref(), Some(TABLE));
    let sql = service
        .object_definition(ID, &trigger[0])
        .await
        .expect("trigger sql");
    assert!(
        sql.to_uppercase().contains("TRIGGER") && sql.contains(TRIGGER),
        "{sql}"
    );

    // A changed object never reaches the SQL.
    let mut fake = trigger[0].clone();
    fake.name = format!("{TRIGGER}; DROP TABLE {TABLE}");
    assert!(service.drop_object(ID, &fake).await.is_err());
    assert!(service.object_definition(ID, &fake).await.is_err());

    if driver != Driver::Sqlite {
        routines(service, driver).await;
    }

    service
        .drop_object(ID, &trigger[0])
        .await
        .expect("drop trigger");
    assert_eq!(object(service, "trigger", TRIGGER).await.len(), 0);
    assert!(listed(service, TABLE).await.is_some());
    if driver == Driver::Postgres {
        let cleanup = format!("DROP FUNCTION {TRIGGER}_fn()");
        service
            .run_query(ID, &cleanup)
            .await
            .expect("drop trigger function");
    }
}

const SAVED_VIEW: &str = "sqlpro_saved_view";
const SAVED_TRIGGER: &str = "sqlpro_saved_trg";

/// A trigger on `table` that runs on `event` and changes nothing.
fn trigger_sql(driver: Driver, event: &str, table: &str) -> String {
    let name = SAVED_TRIGGER;
    match driver {
        Driver::Sqlite => {
            format!("CREATE TRIGGER {name} AFTER {event} ON {table} BEGIN SELECT 1; END")
        }
        Driver::Mssql => {
            format!("CREATE TRIGGER {name} ON {table} AFTER {event} AS SET NOCOUNT ON")
        }
        Driver::Mysql => format!(
            "CREATE TRIGGER {name} BEFORE {event} ON {table} FOR EACH ROW SET NEW.note = NEW.note"
        ),
        Driver::Postgres => format!(
            "CREATE TRIGGER {name} BEFORE {event} ON {table} \
                FOR EACH ROW EXECUTE FUNCTION {name}_fn()"
        ),
    }
}

/// The SQL the object has now, in upper case.
async fn saved_sql(service: &ConnectionService, object: &DbObject) -> String {
    service
        .object_definition(ID, object)
        .await
        .expect("saved sql")
        .to_uppercase()
}

/// Creates a view and a trigger from the editor, changes them, and checks that a
/// change that fails keeps the old one.
async fn save_objects(service: &ConnectionService, driver: Driver) {
    // Only a CREATE may go this way.
    let drop = format!("DROP TABLE {TABLE}");
    assert!(service.save_object(ID, None, &drop).await.is_err());

    let view_sql = format!("CREATE VIEW {SAVED_VIEW} AS SELECT id FROM {TABLE}");
    service
        .save_object(ID, None, &view_sql)
        .await
        .expect("create view");
    let view = listed(service, SAVED_VIEW).await.expect("view is listed");
    let view = DbObject {
        kind: "view".to_owned(),
        schema: view.schema.clone(),
        name: view.name.clone(),
        table: None,
        args: None,
    };
    let changed = format!("CREATE VIEW {SAVED_VIEW} AS SELECT id, born FROM {TABLE}");
    service
        .save_object(ID, Some(&view), &changed)
        .await
        .expect("change view");
    assert!(saved_sql(service, &view).await.contains("BORN"));
    let broken = format!("CREATE VIEW {SAVED_VIEW} AS SELEC id FROM {TABLE}");
    assert!(service.save_object(ID, Some(&view), &broken).await.is_err());
    assert!(saved_sql(service, &view).await.contains("BORN"));

    if driver == Driver::Postgres {
        let function = format!(
            "CREATE FUNCTION {SAVED_TRIGGER}_fn() RETURNS trigger LANGUAGE plpgsql \
                AS $$ BEGIN RETURN NEW; END $$"
        );
        service
            .save_object(ID, None, &function)
            .await
            .expect("create trigger function");
    }
    service
        .save_object(ID, None, &trigger_sql(driver, "INSERT", TABLE))
        .await
        .expect("create trigger");
    let trigger = object(service, "trigger", SAVED_TRIGGER).await;
    assert_eq!(trigger.len(), 1);
    service
        .save_object(ID, Some(&trigger[0]), &trigger_sql(driver, "UPDATE", TABLE))
        .await
        .expect("change trigger");
    assert!(saved_sql(service, &trigger[0]).await.contains("UPDATE"));
    // A new trigger on a table that is not there fails. The old one must stay.
    let broken = trigger_sql(driver, "INSERT", "sqlpro_nope");
    assert!(
        service
            .save_object(ID, Some(&trigger[0]), &broken)
            .await
            .is_err()
    );
    assert_eq!(object(service, "trigger", SAVED_TRIGGER).await.len(), 1);
    assert!(saved_sql(service, &trigger[0]).await.contains("UPDATE"));

    service
        .drop_object(ID, &trigger[0])
        .await
        .expect("drop saved trigger");
    let view = listed(service, SAVED_VIEW).await.expect("view is listed");
    service
        .drop_table(ID, &view)
        .await
        .expect("drop saved view");
    if driver == Driver::Postgres {
        let cleanup = format!("DROP FUNCTION {SAVED_TRIGGER}_fn()");
        service
            .run_query(ID, &cleanup)
            .await
            .expect("drop saved trigger function");
    }
}

fn new_column(spec: ColumnSpec, primary_key: bool, auto_increment: bool) -> NewColumn {
    NewColumn {
        spec,
        primary_key,
        auto_increment,
    }
}

/// Creates a table, adds a row, renames the table and reads the row under the new name.
async fn create_and_rename(service: &ConnectionService) {
    let cleanup = format!("DROP TABLE IF EXISTS {MADE}");
    service.run_query(ID, &cleanup).await.expect("cleanup");
    // Left over from a run that failed.
    if let Some(old) = listed(service, RENAMED).await {
        service.drop_table(ID, &old).await.expect("drop old");
    }

    // Bad input never makes a table.
    let id = || new_column(spec("id", "INTEGER", false, None), true, true);
    let label = || {
        new_column(
            spec("label", "VARCHAR(40)", true, Some("'none'")),
            false,
            false,
        )
    };
    assert!(service.create_table(ID, MADE, &[]).await.is_err());
    assert!(service.create_table(ID, " ", &[id()]).await.is_err());
    let nameless = new_column(spec("", "INTEGER", true, None), false, false);
    assert!(service.create_table(ID, MADE, &[nameless]).await.is_err());
    // Auto increment needs the primary key, and a key of one column.
    let loose = new_column(spec("n", "INTEGER", true, None), false, true);
    assert!(service.create_table(ID, MADE, &[loose]).await.is_err());
    let second = new_column(spec("n", "INTEGER", false, None), true, false);
    assert!(
        service
            .create_table(ID, MADE, &[id(), second])
            .await
            .is_err()
    );
    assert!(listed(service, MADE).await.is_none());

    // A key of two columns. A key column is NOT NULL even when the form says it may be NULL.
    let first = new_column(spec("a", "INTEGER", true, None), true, false);
    let second = new_column(spec("b", "INTEGER", true, None), true, false);
    service
        .create_table(ID, MADE, &[first, second, label()])
        .await
        .expect("create with two key columns");
    let table = listed(service, MADE).await.expect("made is listed");
    let columns = page(service, &table).await.columns;
    assert!(columns[0].primary_key && columns[1].primary_key && !columns[2].primary_key);
    assert!(!columns[0].nullable && !columns[1].nullable && columns[2].nullable);
    service.drop_table(ID, &table).await.expect("drop made");

    service
        .create_table(ID, MADE, &[id(), label()])
        .await
        .expect("create table");
    // The name is taken now.
    assert!(service.create_table(ID, MADE, &[id()]).await.is_err());
    let table = listed(service, MADE).await.expect("made is listed");
    assert_eq!(table.kind, "table");
    // The database gives the id and the default.
    for name in [Some("first"), None] {
        let row = match name {
            Some(name) => values(&[("label", Some(name))]),
            None => Values::new(),
        };
        service.insert_row(ID, &table, row).await.expect("insert");
    }
    let loaded = page(service, &table).await;
    assert!(loaded.columns[0].primary_key);
    assert_eq!(loaded.rows[0], vec![text("1"), text("first")]);
    assert_eq!(loaded.rows[1], vec![text("2"), text("none")]);

    assert!(service.rename_table(ID, &table, "").await.is_err());
    // A name that is not a table never reaches the SQL.
    let mut unknown = table.clone();
    unknown.name = format!("{TABLE}; DROP TABLE {TABLE}");
    assert!(service.rename_table(ID, &unknown, RENAMED).await.is_err());
    // The name of another table is refused by the database.
    assert!(service.rename_table(ID, &table, TABLE).await.is_err());

    service
        .rename_table(ID, &table, RENAMED)
        .await
        .expect("rename table");
    assert!(listed(service, MADE).await.is_none());
    let renamed = listed(service, RENAMED).await.expect("renamed is listed");
    assert_eq!(renamed.schema, table.schema);
    assert_eq!(page(service, &renamed).await.rows, loaded.rows);
    assert!(service.rename_table(ID, &table, RENAMED).await.is_err());

    service
        .drop_table(ID, &renamed)
        .await
        .expect("drop renamed");
    assert!(listed(service, RENAMED).await.is_none());
    assert!(listed(service, TABLE).await.is_some());
}

/// Dumps the database to a file. Expects the test table with the name O'Brien in it.
async fn dump(service: &ConnectionService, driver: Driver, database: &str) {
    let folder = std::env::temp_dir();
    let path = temp_path("dump", "sql");
    if driver == Driver::Mssql {
        let error = service
            .dump_database(ID, &path)
            .await
            .expect_err("no backup");
        assert!(error.to_string().contains("not supported"), "{error}");
        assert!(!path.exists());
        return;
    }
    match service.dump_database(ID, &path).await {
        Err(error) if error.to_string().contains("was not found") => {
            println!("NOTE: the dump check for {driver:?} is skipped: {error}");
            return;
        }
        result => result.expect("dump"),
    }
    let sql = std::fs::read_to_string(&path).expect("dump file");
    if driver == Driver::Postgres {
        assert!(sql.contains("FROM stdin;"), "the dump has COPY rows");
        restore(service, &path, database).await;
    }
    std::fs::remove_file(&path).expect("remove dump");
    assert!(
        sql.lines()
            .any(|line| line.contains("CREATE TABLE") && line.contains(TABLE)),
        "no CREATE TABLE in the dump"
    );
    assert!(sql.contains("Brien") && sql.contains("2020-01-31"));
    // The side file of the dump is gone.
    assert!(!folder.join(format!("{}.part", path.display())).exists());

    // A folder that does not exist: an error, and no file.
    let missing = folder.join("sqlpro-no-such-folder").join("dump.sql");
    assert!(service.dump_database(ID, &missing).await.is_err());
    assert!(!missing.exists());
    assert!(!folder.join("sqlpro-no-such-folder").exists());
}

/// Imports a `pg_dump` backup into a new database, with its `COPY … FROM stdin` rows and
/// psql commands. Expects the two rows that `exercise` inserts. Goes back to the test database.
async fn restore(service: &ConnectionService, path: &std::path::Path, back: &str) {
    // A leftover of an earlier failed run is removed first.
    let _ = service.drop_database(ID, RESTORED).await;
    let created = service.create_database(ID, RESTORED, "", "");
    created.await.expect("create restore database");
    service
        .use_database(ID, RESTORED.to_owned())
        .await
        .expect("use restore database");
    let imported = service.import_sql(ID, path).await;
    assert!(imported.expect("import the pg_dump file") > 0);
    let table = listed(service, TABLE).await.expect("the restored table");
    let rows = page(service, &table).await.rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][1].as_deref(), Some("O'Brien"));
    assert_eq!(rows[1][3].as_deref(), Some("line 1\nline 2"));
    // The dump empties the search path. The import sets it back, so plain names still work.
    let count = format!("SELECT count(*) FROM {TABLE}");
    let found = service.run_query(ID, &count).await.expect("plain name");
    assert_eq!(found[0].rows[0][0].as_deref(), Some("2"));
    service
        .use_database(ID, back.to_owned())
        .await
        .expect("back to the test database");
    service
        .drop_database(ID, RESTORED)
        .await
        .expect("drop restore database");
}

/// Exports the test table as SQL and as CSV. Expects the two rows that `exercise` inserts.
async fn export(service: &ConnectionService, table: &TableRef) {
    let path = temp_path("export", "sql");
    let mut request = ExportRequest {
        tables: vec![table.clone()],
        format: ExportFormat::Sql,
        structure: true,
        data: true,
        columns: Vec::new(),
        path: path.clone(),
    };
    service
        .export_tables(ID, &request)
        .await
        .expect("export sql");
    let sql = std::fs::read_to_string(&path).expect("sql file");
    assert!(sql.contains("CREATE TABLE"), "no CREATE TABLE in {sql}");
    assert!(
        sql.contains("'O''Brien'"),
        "the quote is not escaped in {sql}"
    );
    assert!(sql.contains("'line 1\nline 2'"));
    assert_eq!(sql.matches("INSERT INTO").count(), 2);
    // NULL stays NULL, it does not become a text.
    assert!(sql.contains(", NULL)"));

    // Only the chosen columns, as CSV with a header row.
    request.format = ExportFormat::Csv;
    request.columns = vec!["note".to_owned(), "id".to_owned()];
    service
        .export_tables(ID, &request)
        .await
        .expect("export csv");
    let csv = std::fs::read_to_string(&path).expect("csv file");
    assert_eq!(csv, "note,id\r\n,1\r\n\"line 1\nline 2\",2\r\n");

    // CSV into a folder: one file per table.
    let out = temp_path("export", "csv");
    std::fs::create_dir_all(&out).expect("folder");
    request.columns = Vec::new();
    request.path = out.clone();
    service
        .export_tables(ID, &request)
        .await
        .expect("export folder");
    let csv = std::fs::read_to_string(out.join(format!("{TABLE}.csv"))).expect("table csv");
    assert!(csv.starts_with("id,name,born,note\r\n1,O'Brien,2020-01-31,\r\n"));

    // JSON and Excel into a folder: one file per table, too.
    for format in [ExportFormat::Json, ExportFormat::Xlsx] {
        request.format = format;
        service
            .export_tables(ID, &request)
            .await
            .expect("export folder");
    }
    let json = std::fs::read_to_string(out.join(format!("{TABLE}.json"))).expect("table json");
    assert!(
        json.starts_with("[\n  {\n    \"id\": \"1\",\n    \"name\": \"O'Brien\""),
        "{json}"
    );
    assert!(json.contains("\"note\": null"));
    assert!(json.contains("\"line 1\\nline 2\""));
    let xlsx = std::fs::read(out.join(format!("{TABLE}.xlsx"))).expect("table xlsx");
    // An xlsx file is a zip file.
    assert!(xlsx.starts_with(b"PK"));

    // Many tables as JSON or Excel need a folder.
    request.path = path.clone();
    request.tables = vec![table.clone(), table.clone()];
    assert!(service.export_tables(ID, &request).await.is_err());
    request.tables = vec![table.clone()];
    request.format = ExportFormat::Csv;

    // An unknown column fails and leaves no file behind.
    std::fs::remove_file(&path).expect("remove export");
    request.path = path.clone();
    request.columns = vec!["nope".to_owned()];
    assert!(service.export_tables(ID, &request).await.is_err());
    assert!(!path.exists());
    std::fs::remove_dir_all(&out).expect("remove export folder");
    export_rows(service, &path);
}

/// Rows the app already has: written as they are, in each format, without reading the table.
fn export_rows(service: &ConnectionService, path: &std::path::Path) {
    let mut rows = RowsExport {
        format: ExportFormat::Sql,
        table: TableRef {
            schema: None,
            name: "picked".to_owned(),
            kind: String::new(),
        },
        columns: vec!["id".to_owned(), "name".to_owned()],
        rows: vec![
            vec![Some("1".to_owned()), Some("O'Brien".to_owned())],
            vec![Some("2".to_owned()), None],
        ],
        path: path.to_path_buf(),
    };
    service.export_rows(ID, &rows).expect("export rows sql");
    let sql = std::fs::read_to_string(path).expect("rows sql");
    assert_eq!(sql.matches("INSERT INTO").count(), 2);
    assert!(
        sql.contains("'O''Brien'"),
        "the quote is not escaped in {sql}"
    );
    assert!(sql.contains("NULL);"));
    rows.format = ExportFormat::Csv;
    service.export_rows(ID, &rows).expect("export rows csv");
    let csv = std::fs::read_to_string(path).expect("rows csv");
    assert_eq!(csv, "id,name\r\n1,O'Brien\r\n2,\r\n");
    rows.format = ExportFormat::Json;
    service.export_rows(ID, &rows).expect("export rows json");
    let json = std::fs::read_to_string(path).expect("rows json");
    assert_eq!(
        json,
        "[\n  {\n    \"id\": \"1\",\n    \"name\": \"O'Brien\"\n  },\n  {\n    \"id\": \"2\",\n    \"name\": null\n  }\n]\n"
    );
    rows.format = ExportFormat::Xlsx;
    service.export_rows(ID, &rows).expect("export rows xlsx");
    assert!(std::fs::read(path).expect("rows xlsx").starts_with(b"PK"));
    std::fs::remove_file(path).expect("remove rows export");
}

/// "Copy Rows As > INSERT Statement": names and values quoted the way each database wants.
fn insert_statements(service: &ConnectionService, driver: Driver) {
    let request = RowsSql {
        table: TableRef {
            schema: Some("app".to_owned()),
            name: "picked".to_owned(),
            kind: String::new(),
        },
        columns: vec!["id".to_owned(), "a\"b`c]d".to_owned()],
        rows: vec![
            vec![text("O'Brien"), text("C:\\dir")],
            vec![text("2"), None],
        ],
    };
    let sql = service.rows_insert_sql(ID, &request).expect("insert sql");
    let expected = match driver {
        Driver::Mysql => concat!(
            "INSERT INTO `app`.`picked` (`id`, `a\"b``c]d`) VALUES ('O''Brien', 'C:\\\\dir');\n",
            "INSERT INTO `app`.`picked` (`id`, `a\"b``c]d`) VALUES ('2', NULL);\n",
        ),
        Driver::Mssql => concat!(
            "INSERT INTO [app].[picked] ([id], [a\"b`c]]d]) VALUES (N'O''Brien', N'C:\\dir');\n",
            "INSERT INTO [app].[picked] ([id], [a\"b`c]]d]) VALUES (N'2', NULL);\n",
        ),
        Driver::Postgres | Driver::Sqlite => concat!(
            "INSERT INTO \"app\".\"picked\" (\"id\", \"a\"\"b`c]d\") VALUES ('O''Brien', 'C:\\dir');\n",
            "INSERT INTO \"app\".\"picked\" (\"id\", \"a\"\"b`c]d\") VALUES ('2', NULL);\n",
        ),
    };
    assert_eq!(sql, expected);
}

fn csv_column(name: &str, field: usize) -> CsvColumn {
    CsvColumn {
        name: name.to_owned(),
        field,
    }
}

/// Exports the test table as SQL and imports the file back under another name.
/// Then imports CSV rows into that copy. Leaves the test table as it was.
async fn import(service: &ConnectionService, table: &TableRef) {
    let path = temp_path("import", "sql");
    let request = ExportRequest {
        tables: vec![table.clone()],
        format: ExportFormat::Sql,
        structure: true,
        data: true,
        columns: Vec::new(),
        path: path.clone(),
    };
    service
        .export_tables(ID, &request)
        .await
        .expect("export for import");
    let sql = std::fs::read_to_string(&path).expect("sql file");
    std::fs::write(&path, sql.replace(TABLE, IMPORTED)).expect("write sql");
    service
        .run_query(ID, &format!("DROP TABLE IF EXISTS {IMPORTED}"))
        .await
        .expect("drop old copy");
    // One CREATE TABLE and two INSERTs.
    assert_eq!(service.import_sql(ID, &path).await.expect("import sql"), 3);
    let copy = listed(service, IMPORTED).await.expect("the copy is listed");
    assert_eq!(
        page(service, &copy).await.rows,
        page(service, table).await.rows
    );

    // A bad statement rolls back the statements before it, and names its line.
    let bad = format!("INSERT INTO {IMPORTED} (id) VALUES (9);\nSELEC nonsense;\n");
    std::fs::write(&path, bad).expect("write bad sql");
    let error = service.import_sql(ID, &path).await.expect_err("bad sql");
    assert!(
        error.to_string().starts_with("Statement at line 2: "),
        "{error}"
    );
    assert_eq!(page(service, &copy).await.total, 2);

    // CSV with a header, fields in another order, quotes and empty fields.
    let csv = temp_path("import", "csv");
    std::fs::write(&csv, "note,id,name\r\n\"a, \"\"b\"\"\",10,\r\n,11,Zed\r\n").expect("csv");
    let preview = ConnectionService::preview_csv(&csv).expect("preview");
    assert_eq!(preview.count, 3);
    assert_eq!(preview.rows[1], vec!["a, \"b\"", "10", ""]);
    let mut request = CsvImport {
        table: copy.clone(),
        path: csv.clone(),
        header: true,
        empty_is_null: true,
        columns: vec![
            csv_column("id", 1),
            csv_column("name", 2),
            csv_column("note", 0),
        ],
    };
    assert_eq!(service.import_csv(ID, &request).await.expect("csv"), 2);
    let rows = page(service, &copy).await.rows;
    assert_eq!(rows[2], vec![text("10"), None, None, text("a, \"b\"")]);
    assert_eq!(rows[3], vec![text("11"), text("Zed"), None, None]);

    // No header, empty text kept. Line 3 repeats the key of line 1, so nothing is kept.
    std::fs::write(&csv, "12,\n13,x\n12,y\n").expect("csv");
    request.header = false;
    request.empty_is_null = false;
    request.columns = vec![csv_column("id", 0), csv_column("name", 1)];
    let error = service
        .import_csv(ID, &request)
        .await
        .expect_err("same key");
    assert!(
        error.to_string().starts_with("Line 3 could not be saved"),
        "{error}"
    );
    assert_eq!(page(service, &copy).await.total, 4);

    std::fs::write(&csv, "12,\n").expect("csv");
    assert_eq!(service.import_csv(ID, &request).await.expect("csv"), 1);
    assert_eq!(page(service, &copy).await.rows[4][1], text(""));

    // A row with too few fields fails before anything runs.
    std::fs::write(&csv, "14\n").expect("csv");
    let error = service
        .import_csv(ID, &request)
        .await
        .expect_err("short row");
    assert_eq!(error.to_string(), "Line 1 has only 1 fields");

    service.drop_table(ID, &copy).await.expect("drop copy");
    std::fs::remove_file(&path).expect("remove sql");
    std::fs::remove_file(&csv).expect("remove csv");
}

/// The one foreign key that `table` holds.
fn relation<'found>(diagram: &'found Diagram, table: &str) -> &'found Relation {
    let mut found = diagram.relations.iter().filter(|key| key.table == table);
    let relation = found.next().expect("the table has a foreign key");
    assert!(found.next().is_none(), "{table} has more than one key");
    relation
}

/// Loads the diagram of a small schema: plain, unique and composite foreign keys, and a view.
async fn diagram(service: &ConnectionService, driver: Driver) {
    let drop = "DROP VIEW IF EXISTS sqlpro_d_view; DROP TABLE IF EXISTS sqlpro_d_short; \
        DROP TABLE IF EXISTS sqlpro_d_page; DROP TABLE IF EXISTS sqlpro_d_edition; \
        DROP TABLE IF EXISTS sqlpro_d_profile; DROP TABLE IF EXISTS sqlpro_d_book; \
        DROP TABLE IF EXISTS sqlpro_d_author";
    // The page key lists its columns in another order than the table does.
    let setup = format!(
        "{drop}; \
        CREATE TABLE sqlpro_d_author (id INTEGER PRIMARY KEY, name VARCHAR(20)); \
        CREATE TABLE sqlpro_d_book (id INTEGER PRIMARY KEY, author_id INTEGER, \
            CONSTRAINT sqlpro_d_book_author FOREIGN KEY (author_id) REFERENCES sqlpro_d_author (id)); \
        CREATE TABLE sqlpro_d_profile (id INTEGER PRIMARY KEY, author_id INTEGER UNIQUE, \
            FOREIGN KEY (author_id) REFERENCES sqlpro_d_author (id)); \
        CREATE TABLE sqlpro_d_edition (book_id INTEGER, number INTEGER, \
            PRIMARY KEY (book_id, number), \
            FOREIGN KEY (book_id) REFERENCES sqlpro_d_book (id)); \
        CREATE TABLE sqlpro_d_page (id INTEGER PRIMARY KEY, number INTEGER, book_id INTEGER, \
            FOREIGN KEY (book_id, number) REFERENCES sqlpro_d_edition (book_id, number))"
    );
    service.run_query(ID, &setup).await.expect("diagram tables");
    let view = "CREATE VIEW sqlpro_d_view AS SELECT id FROM sqlpro_d_book";
    service.run_query(ID, view).await.expect("diagram view");
    if driver == Driver::Sqlite {
        // No parent column, and the parent name in other letters.
        let short = "CREATE TABLE sqlpro_d_short (id INTEGER PRIMARY KEY, \
            Author_Id INTEGER REFERENCES SQLPRO_D_AUTHOR, FOREIGN KEY (AUTHOR_ID) REFERENCES sqlpro_d_author (ID))";
        service.run_query(ID, short).await.expect("short key");
    }

    let diagram = service.diagram(ID).await.expect("diagram");
    let book = diagram
        .tables
        .iter()
        .find(|one| one.table.name == "sqlpro_d_book");
    let book = book.expect("the book table is in the diagram");
    let mut names = Vec::new();
    for column in &book.columns {
        names.push(column.name.as_str());
    }
    assert_eq!(names, ["id", "author_id"]);
    assert!(book.columns[0].primary_key);
    // Views are left out.
    assert!(diagram.tables.iter().all(|one| one.table.kind == "table"));
    assert!(
        diagram
            .tables
            .iter()
            .all(|one| one.table.name != "sqlpro_d_view")
    );

    let plain = relation(&diagram, "sqlpro_d_book");
    let name = if driver == Driver::Sqlite {
        ""
    } else {
        "sqlpro_d_book_author"
    };
    assert_eq!(plain.name, name);
    assert_eq!(plain.schema, book.table.schema);
    assert_eq!(plain.ref_schema, book.table.schema);
    assert_eq!(plain.columns, ["author_id"]);
    assert_eq!(plain.ref_table, "sqlpro_d_author");
    assert_eq!(plain.ref_columns, ["id"]);
    assert!(!plain.unique);

    let unique = relation(&diagram, "sqlpro_d_profile");
    assert_eq!(unique.columns, ["author_id"]);
    assert_eq!(unique.ref_table, "sqlpro_d_author");
    assert!(unique.unique);

    // One column of a primary key with two columns is not unique alone.
    assert!(!relation(&diagram, "sqlpro_d_edition").unique);

    let composite = relation(&diagram, "sqlpro_d_page");
    assert_eq!(composite.columns, ["book_id", "number"]);
    assert_eq!(composite.ref_table, "sqlpro_d_edition");
    assert_eq!(composite.ref_columns, ["book_id", "number"]);
    assert!(!composite.unique);

    if driver == Driver::Sqlite {
        // Both keys come back with the real names of the tables and columns.
        let short: Vec<&Relation> = diagram
            .relations
            .iter()
            .filter(|key| key.table == "sqlpro_d_short")
            .collect();
        assert_eq!(short.len(), 2);
        for key in short {
            assert_eq!(key.columns, ["Author_Id"]);
            assert_eq!(key.ref_table, "sqlpro_d_author");
            assert_eq!(key.ref_columns, ["id"]);
        }
    }

    // The frontend reads these names.
    let json = serde_json::to_value(plain).expect("json");
    for field in [
        "name",
        "schema",
        "table",
        "columns",
        "refSchema",
        "refTable",
        "refColumns",
        "unique",
    ] {
        assert!(json.get(field).is_some(), "no {field} in the JSON");
    }
    service
        .run_query(ID, drop)
        .await
        .expect("drop diagram tables");
}

/// Writes a file, and fails for a folder that does not exist.
fn save_image() {
    let path = temp_path("image", "png");
    let folder = std::env::temp_dir();
    ConnectionService::save_image(&path, &[1, 2, 3]).expect("save image");
    assert_eq!(std::fs::read(&path).expect("image file"), [1, 2, 3]);
    std::fs::remove_file(&path).expect("remove image");
    let missing = folder.join("sqlpro-no-such-folder").join("image.png");
    assert!(ConnectionService::save_image(&missing, &[1]).is_err());
}

fn new_user(name: &str, password: &str, access: AccessLevel) -> NewUser {
    NewUser {
        name: name.to_owned(),
        host: None,
        password: password.to_owned(),
        access,
    }
}

async fn find_user(service: &ConnectionService, name: &str) -> Option<DbUser> {
    let list = service.list_users(ID).await.expect("list users");
    list.users.into_iter().find(|user| user.name == name)
}

async fn access_of(service: &ConnectionService, name: &str) -> Option<AccessLevel> {
    find_user(service, name).await.map(|user| user.access)
}

/// Logs in as another account, on connection `USER_ID`.
async fn login(
    service: &ConnectionService,
    config: &ConnectionConfig,
    name: &str,
    password: &str,
) -> bool {
    let mut config = config.clone();
    name.clone_into(&mut config.user);
    password.clone_into(&mut config.password);
    service.connect(USER_ID.to_owned(), config).await.is_ok()
}

/// Logs in as the test account and tries to read and to add a row.
/// Gives (can read, can write). A failed login counts as "cannot read".
async fn rights(
    service: &ConnectionService,
    config: &ConnectionConfig,
    name: &str,
    password: &str,
) -> (bool, bool) {
    if !login(service, config, name, password).await {
        return (false, false);
    }
    let select = format!("SELECT * FROM {TABLE}");
    let insert = format!("INSERT INTO {TABLE} (id) VALUES (50)");
    let delete = format!("DELETE FROM {TABLE} WHERE id = 50");
    let can_read = service.run_query(USER_ID, &select).await.is_ok();
    let can_write = service.run_query(USER_ID, &insert).await.is_ok()
        && service.run_query(USER_ID, &delete).await.is_ok();
    // Nobody with these levels may change the table itself.
    let alter = format!("ALTER TABLE {TABLE} ADD COLUMN hacked INTEGER");
    assert!(service.run_query(USER_ID, &alter).await.is_err());
    service.disconnect(USER_ID).await;
    (can_read, can_write)
}

/// `SQLite` has no accounts, and the app does not manage SQL Server accounts yet:
/// an empty list, and every change is an error.
async fn no_users(service: &ConnectionService) {
    let list = service.list_users(ID).await.expect("list users");
    assert!(!list.can_manage);
    assert!(list.users.is_empty());
    let user = new_user(USER, PASSWORD, AccessLevel::Read);
    assert!(service.create_user(ID, &user).await.is_err());
    assert!(service.drop_user(ID, USER, None).await.is_err());
    let changed = service.set_user_access(ID, USER, None, AccessLevel::Read);
    assert!(changed.await.is_err());
    let changed = service.set_user_password(ID, USER, None, PASSWORD);
    assert!(changed.await.is_err());
    let changed = service.set_user_query_stats(ID, USER, None, true);
    assert!(changed.await.is_err());
}

/// Bad input and forbidden changes. None of them may change anything.
async fn user_refusals(service: &ConnectionService, me: &DbUser, table: &TableRef) {
    let host = me.host.as_deref();
    let error = service.drop_user(ID, &me.name, host).await;
    let error = error.expect_err("dropping yourself is refused");
    assert!(error.to_string().contains("own account"), "{error}");
    let changed = service.set_user_access(ID, &me.name, host, AccessLevel::None);
    assert!(
        changed.await.is_err(),
        "changing your own access is refused"
    );
    let still_me = find_user(service, &me.name).await.expect("still listed");
    assert_eq!(still_me.access, me.access);

    for (name, password) in [
        ("", "x"),
        ("a\nb", "x"),
        ("a\0b", "x"),
        (USER, ""),
        (USER, "a\nb"),
    ] {
        let user = new_user(name, password, AccessLevel::None);
        assert!(service.create_user(ID, &user).await.is_err(), "{name:?}");
    }
    let long_name = "x".repeat(64);
    let user = new_user(&long_name, PASSWORD, AccessLevel::None);
    assert!(service.create_user(ID, &user).await.is_err());
    // Only none, read and write can be set.
    let user = new_user("sqlpro_never", PASSWORD, AccessLevel::Admin);
    assert!(service.create_user(ID, &user).await.is_err());
    assert!(find_user(service, "sqlpro_never").await.is_none());
    let changed = service.set_user_access(ID, USER, None, AccessLevel::Custom);
    assert!(changed.await.is_err());
    let changed = service.set_user_password(ID, USER, None, "");
    assert!(changed.await.is_err());

    // A name that is not an account never reaches the SQL.
    let unknown = format!("{USER}'; DROP TABLE {TABLE}; --");
    assert!(service.drop_user(ID, &unknown, None).await.is_err());
    let changed = service.set_user_access(ID, &unknown, None, AccessLevel::Read);
    assert!(changed.await.is_err());
    let changed = service.set_user_password(ID, &unknown, None, PASSWORD);
    assert!(changed.await.is_err());

    // A second account with the same name: the error must not show the password.
    let user = new_user(USER, "secret-Word-77", AccessLevel::None);
    let error = service.create_user(ID, &user).await.expect_err("duplicate");
    assert!(!error.to_string().contains("secret-Word-77"), "{error}");
    assert_eq!(page(service, table).await.total, 2);
}

/// Rights that are not one of the three levels: single tables, and rights on everything.
async fn special_rights(service: &ConnectionService, driver: Driver) {
    let account = if driver == Driver::Mysql {
        format!("'{USER}'@'%'")
    } else {
        USER.to_owned()
    };
    // A right on one table that fits no level is `custom`. Setting a level removes it.
    let grant = format!("GRANT INSERT ON {TABLE} TO {account}");
    service.run_query(ID, &grant).await.expect("table grant");
    assert_eq!(access_of(service, USER).await, Some(AccessLevel::Custom));
    let changed = service.set_user_access(ID, USER, None, AccessLevel::None);
    changed.await.expect("custom to none");
    assert_eq!(access_of(service, USER).await, Some(AccessLevel::None));

    // Rights that reach past this database cannot be changed per database.
    let (give, take) = if driver == Driver::Mysql {
        (
            format!("GRANT SELECT ON *.* TO {account}"),
            format!("REVOKE SELECT ON *.* FROM {account}"),
        )
    } else {
        (
            format!("ALTER ROLE {account} SUPERUSER"),
            format!("ALTER ROLE {account} NOSUPERUSER"),
        )
    };
    service.run_query(ID, &give).await.expect("global right");
    let changed = service.set_user_access(ID, USER, None, AccessLevel::None);
    assert!(changed.await.is_err(), "global rights are refused");
    service.run_query(ID, &take).await.expect("take it back");
    assert_eq!(access_of(service, USER).await, Some(AccessLevel::None));
}

/// The right to read the query stats: give it, take it away, and the cases that are refused.
async fn query_stats_right(service: &ConnectionService, driver: Driver, me: &DbUser) {
    let stats_of = async |name: &str| {
        let user = find_user(service, name).await.expect("listed");
        (user.query_stats, user.query_stats_fixed)
    };
    assert_eq!(stats_of(USER).await, (false, false));
    let changed = service.set_user_query_stats(ID, USER, None, true);
    changed.await.expect("give query stats");
    assert_eq!(stats_of(USER).await, (true, false));
    // Asking again for what is already there changes nothing.
    let changed = service.set_user_query_stats(ID, USER, None, true);
    changed.await.expect("give again");
    let changed = service.set_user_query_stats(ID, USER, None, false);
    changed.await.expect("take query stats");
    assert_eq!(stats_of(USER).await, (false, false));

    // The admin has it through a wider right, so it cannot be taken away here.
    assert!(me.query_stats && me.query_stats_fixed);
    let host = me.host.as_deref();
    let changed = service.set_user_query_stats(ID, &me.name, host, false);
    assert!(changed.await.is_err(), "a wider right is refused");
    let unknown = format!("{USER}'; DROP TABLE {TABLE}; --");
    let changed = service.set_user_query_stats(ID, &unknown, None, true);
    assert!(changed.await.is_err());

    let account = if driver == Driver::Mysql {
        format!("'{USER}'@'%'")
    } else {
        USER.to_owned()
    };
    let (give, take) = if driver == Driver::Mysql {
        (
            format!("GRANT SELECT ON *.* TO {account}"),
            format!("REVOKE SELECT ON *.* FROM {account}"),
        )
    } else {
        (
            format!("ALTER ROLE {account} SUPERUSER"),
            format!("ALTER ROLE {account} NOSUPERUSER"),
        )
    };
    service.run_query(ID, &give).await.expect("wider right");
    assert_eq!(stats_of(USER).await, (true, true));
    let changed = service.set_user_query_stats(ID, USER, None, false);
    assert!(changed.await.is_err());
    service.run_query(ID, &take).await.expect("take it back");
    assert_eq!(stats_of(USER).await, (false, false));
}

/// `MySQL` with no database chosen: the list works, but a level cannot be set.
async fn no_database(service: &ConnectionService, config: &ConnectionConfig) {
    let mut plain = config.clone();
    plain.database = String::new();
    let connected = service.connect(USER_ID.to_owned(), plain).await;
    connected.expect("connect without a database");
    let list = service.list_users(USER_ID).await.expect("list users");
    assert!(list.can_manage);
    assert!(list.users.iter().any(|user| user.name == USER));

    let user = new_user("sqlpro_never", PASSWORD, AccessLevel::Read);
    let error = service.create_user(USER_ID, &user).await;
    let error = error.expect_err("no database, no level");
    assert!(error.to_string().contains("Choose a database"), "{error}");
    // The account was not created half way.
    assert!(find_user(service, "sqlpro_never").await.is_none());
    let changed = service.set_user_access(USER_ID, USER, None, AccessLevel::Read);
    assert!(changed.await.is_err());
    service.disconnect(USER_ID).await;
    assert_eq!(access_of(service, USER).await, Some(AccessLevel::None));
}

/// Changes the password of the test account: first as the admin, then as the account itself.
/// Also checks that an account without the right cannot manage users.
async fn passwords(service: &ConnectionService, config: &ConnectionConfig, me: &DbUser) {
    let mysql = config.driver == Driver::Mysql;
    let mut plain = config.clone();
    if mysql {
        // MySQL lets an account without rights log in only when no database is chosen.
        plain.database = String::new();
    }
    let changed = service.set_user_password(ID, USER, None, TRICKY_PASSWORD);
    changed.await.expect("new password");
    assert!(login(service, &plain, USER, TRICKY_PASSWORD).await);

    // Now we are the test account, on its own connection.
    let list = service.list_users(USER_ID).await.expect("list as user");
    assert!(!list.can_manage);
    let user = new_user("sqlpro_never", PASSWORD, AccessLevel::None);
    assert!(service.create_user(USER_ID, &user).await.is_err());
    let host = me.host.as_deref();
    assert!(service.drop_user(USER_ID, &me.name, host).await.is_err());
    let changed = service.set_user_access(USER_ID, &me.name, host, AccessLevel::None);
    assert!(changed.await.is_err());
    let changed = service.set_user_password(USER_ID, &me.name, host, "stolen-1");
    assert!(changed.await.is_err());
    let changed = service.set_user_query_stats(USER_ID, USER, None, true);
    assert!(changed.await.is_err());
    // Everyone may change their own password. The connection keeps working.
    let own = service.set_user_password(USER_ID, USER, None, PASSWORD);
    own.await.expect("own password");
    assert!(service.run_query(USER_ID, "SELECT 1").await.is_ok());
    service.disconnect(USER_ID).await;
    assert!(find_user(service, "sqlpro_never").await.is_none());

    // PostgreSQL may run with trust auth, so a wrong password is only checked on MySQL.
    if mysql {
        assert!(!login(service, &plain, USER, TRICKY_PASSWORD).await);
        assert!(login(service, &plain, USER, PASSWORD).await);
        service.disconnect(USER_ID).await;
    }
    // The admin connection still works after all this.
    assert!(service.list_users(ID).await.expect("list").can_manage);
}

/// Creates, changes and drops accounts. Expects the test table with two rows.
async fn users(service: &ConnectionService, config: &ConnectionConfig, table: &TableRef) {
    let driver = config.driver;
    if matches!(driver, Driver::Sqlite | Driver::Mssql) {
        no_users(service).await;
        return;
    }
    // An earlier run that failed may have left its accounts behind.
    for name in [USER, TRICKY_USER] {
        if find_user(service, name).await.is_some() {
            service.drop_user(ID, name, None).await.expect("leftover");
        }
    }

    let list = service.list_users(ID).await.expect("list users");
    assert!(list.can_manage);
    let mut current = list.users.iter().filter(|user| user.current);
    let me = current.next().expect("the current account is listed");
    assert!(current.next().is_none(), "only one account is current");
    assert_eq!(me.name, config.user);
    assert_eq!(me.access, AccessLevel::Admin);
    assert_eq!(me.host.is_some(), driver == Driver::Mysql);

    // Read: SELECT works, INSERT does not.
    let user = new_user(USER, PASSWORD, AccessLevel::Read);
    service.create_user(ID, &user).await.expect("create user");
    let created = find_user(service, USER)
        .await
        .expect("the new user is listed");
    assert_eq!(created.access, AccessLevel::Read);
    assert!(!created.current);
    assert_eq!(rights(service, config, USER, PASSWORD).await, (true, false));

    let changed = service.set_user_access(ID, USER, None, AccessLevel::Write);
    changed.await.expect("to write");
    assert_eq!(access_of(service, USER).await, Some(AccessLevel::Write));
    assert_eq!(rights(service, config, USER, PASSWORD).await, (true, true));

    let changed = service.set_user_access(ID, USER, None, AccessLevel::None);
    changed.await.expect("to none");
    assert_eq!(access_of(service, USER).await, Some(AccessLevel::None));
    assert_eq!(
        rights(service, config, USER, PASSWORD).await,
        (false, false)
    );

    user_refusals(service, me, table).await;
    special_rights(service, driver).await;
    query_stats_right(service, driver, me).await;

    passwords(service, config, me).await;
    if driver == Driver::Mysql {
        no_database(service, config).await;
    }

    // A name and a password full of SQL: plain text, and nothing else is harmed.
    if driver == Driver::Mysql {
        // In this mode a backslash is plain text. The password must still be the same.
        let mode = "SET @sqlpro_mode = @@sql_mode; \
            SET SESSION sql_mode = CONCAT(@@sql_mode, ',NO_BACKSLASH_ESCAPES')";
        service.run_query(ID, mode).await.expect("sql_mode");
    }
    let tricky = new_user(TRICKY_USER, TRICKY_PASSWORD, AccessLevel::Write);
    service.create_user(ID, &tricky).await.expect("tricky user");
    assert_eq!(
        access_of(service, TRICKY_USER).await,
        Some(AccessLevel::Write)
    );
    let found = rights(service, config, TRICKY_USER, TRICKY_PASSWORD).await;
    assert_eq!(found, (true, true));
    if driver == Driver::Mysql {
        assert!(!login(service, config, TRICKY_USER, "p").await);
        let mode = "SET SESSION sql_mode = @sqlpro_mode";
        service.run_query(ID, mode).await.expect("sql_mode back");
    }
    assert_eq!(page(service, table).await.total, 2);
    assert_eq!(access_of(service, USER).await, Some(AccessLevel::None));

    for name in [TRICKY_USER, USER] {
        service.drop_user(ID, name, None).await.expect("drop user");
        assert!(find_user(service, name).await.is_none());
    }
    assert!(service.drop_user(ID, USER, None).await.is_err());
}

fn update(id: &str, changes: &[(&str, Option<&str>)]) -> RowUpdate {
    RowUpdate {
        key: values(&[("id", Some(id))]),
        changes: values(changes),
    }
}

/// Deletes the row with this id through `save_changes`. Exactly one row must go.
async fn delete_row(service: &ConnectionService, table: &TableRef, id: &str) {
    let keys = vec![values(&[("id", Some(id))])];
    let saved = service.save_changes(ID, table, Vec::new(), Vec::new(), keys);
    assert_eq!(saved.await.expect("delete").deleted, 1, "row {id}");
}

/// Deletes through `save_changes`. Expects rows 1, 2 and 3, and removes row 3.
async fn save_deletes(service: &ConnectionService, table: &TableRef) {
    let before = page(service, table).await.rows;
    let key_3 = || vec![values(&[("id", Some("3"))])];
    let lost = || vec![update("1", &[("name", Some("lost"))])];

    // The delete of row 3 runs first. A later change fails, so row 3 must come back.
    let inserts = vec![values(&[("id", Some("1"))])];
    let saved = service.save_changes(ID, table, Vec::new(), inserts, key_3());
    let error = saved.await.expect_err("rollback").to_string();
    assert!(error.starts_with("New row 1 could not be saved"), "{error}");
    assert_eq!(page(service, table).await.rows, before);

    // A delete without the key. It must never touch all rows.
    let deletes = vec![values(&[("id", Some("3"))]), Values::new()];
    let saved = service.save_changes(ID, table, lost(), Vec::new(), deletes);
    let error = saved.await.expect_err("no key").to_string();
    assert!(
        error.starts_with("Deleted row 2 could not be saved"),
        "{error}"
    );
    assert_eq!(page(service, table).await.rows, before);

    // One batch with all three kinds. The delete goes first, so the new row can take key 3.
    let updates = vec![update("1", &[("note", Some("x"))])];
    let inserts = vec![values(&[("id", Some("3")), ("name", Some("Di"))])];
    let saved = service.save_changes(ID, table, updates, inserts, key_3());
    let expected = SaveResult {
        updated: 1,
        inserted: 1,
        deleted: 1,
    };
    assert_eq!(saved.await.expect("mixed batch"), expected);
    let mixed = page(service, table).await.rows;
    assert_eq!(mixed[0][3], text("x"));
    assert_eq!(mixed[2], vec![text("3"), text("Di"), None, None]);

    // The connection is not stuck in a transaction: a later delete still lands.
    delete_row(service, table, "3").await;
    // A row that is gone already is not an error. It is only not counted.
    let saved = service.save_changes(ID, table, Vec::new(), Vec::new(), key_3());
    assert_eq!(saved.await.expect("delete again"), SaveResult::default());
}

/// Saves batches of changes. Expects rows 1 and 2, and leaves them as they were.
async fn save_changes(service: &ConnectionService, table: &TableRef) {
    let before = page(service, table).await.rows;
    // Quotes and a backslash must be stored as they are.
    let tricky = r#"a'b"c\d"#;

    let updates = vec![
        update("1", &[("note", Some(tricky))]),
        // No changes: this one is skipped.
        update("2", &[]),
        update("2", &[("name", Some("Bo")), ("note", None)]),
    ];
    let inserts = vec![values(&[("id", Some("3")), ("name", Some("Cy"))])];
    let saved = service
        .save_changes(ID, table, updates, inserts, Vec::new())
        .await;
    let expected = SaveResult {
        updated: 2,
        inserted: 1,
        deleted: 0,
    };
    assert_eq!(saved.expect("save"), expected);
    let after = page(service, table).await.rows;
    assert_eq!(after.len(), 3);
    assert_eq!(after[0][3], text(tricky));
    assert_eq!(after[1], vec![text("2"), text("Bo"), None, None]);
    assert_eq!(after[2], vec![text("3"), text("Cy"), None, None]);

    // A later change fails: the earlier ones of the same batch are undone.
    let failing: [(Vec<RowUpdate>, Vec<Values>, &str); 4] = [
        // A second row with the same primary key.
        (
            vec![update("1", &[("name", Some("lost"))])],
            vec![values(&[("id", Some("4"))]), values(&[("id", Some("4"))])],
            "New row 2 could not be saved",
        ),
        // A value the database refuses, in the second update.
        (
            vec![
                update("1", &[("name", Some("lost"))]),
                update("2", &[("id", Some("1"))]),
            ],
            vec![values(&[("id", Some("5"))])],
            "Row 2 could not be saved",
        ),
        // An unknown column.
        (
            vec![update("1", &[("name", Some("lost"))])],
            vec![values(&[("nope", Some("x"))])],
            "New row 1 could not be saved",
        ),
        // An update without the key. It must never touch all rows.
        (
            vec![
                update("1", &[("name", Some("lost"))]),
                RowUpdate {
                    key: Values::new(),
                    changes: values(&[("name", Some("everyone"))]),
                },
            ],
            Vec::new(),
            "Row 2 could not be saved",
        ),
    ];
    for (updates, inserts, message) in failing {
        let saved = service
            .save_changes(ID, table, updates, inserts, Vec::new())
            .await;
        let error = saved.expect_err(message).to_string();
        assert!(error.starts_with(message), "{error}");
        assert_eq!(page(service, table).await.rows, after, "{message}");
    }

    let saved = service.save_changes(ID, table, Vec::new(), Vec::new(), Vec::new());
    assert_eq!(saved.await.expect("empty batch"), SaveResult::default());

    save_deletes(service, table).await;

    // Put rows 1 and 2 back, also through one batch.
    let updates = vec![
        update("1", &[("note", None)]),
        update("2", &[("name", None), ("note", Some("line 1\nline 2"))]),
    ];
    let saved = service
        .save_changes(ID, table, updates, Vec::new(), Vec::new())
        .await;
    assert_eq!(saved.expect("restore").updated, 2);
    assert_eq!(page(service, table).await.rows, before);
}

/// Copies the connection. The copy must work on its own. Leaves `ID` connected.
async fn clone_connection(
    service: &ConnectionService,
    config: &ConnectionConfig,
    table: &TableRef,
) {
    let cloned = service.clone_connection(ID, CLONE_ID.to_owned()).await;
    cloned.expect("clone");
    let tables = service.tables(CLONE_ID).await.expect("tables of the clone");
    assert!(tables.iter().any(|listed| listed.name == TABLE));
    let request = request(table, &[], "");
    let loaded = service.browse(CLONE_ID, &request).await.expect("browse");
    assert_eq!(loaded.total, 2);

    // The copy lives on when the first connection is closed.
    service.disconnect(ID).await;
    assert!(service.tables(ID).await.is_err());
    let loaded = service.browse(CLONE_ID, &request).await.expect("browse");
    assert_eq!(loaded.total, 2);

    // An id that is not connected cannot be copied.
    assert!(
        service
            .clone_connection(ID, "other".to_owned())
            .await
            .is_err()
    );
    assert!(service.tables("other").await.is_err());

    let connected = service.connect(ID.to_owned(), config.clone()).await;
    connected.expect("connect again");
    service.disconnect(CLONE_ID).await;
    assert!(service.tables(CLONE_ID).await.is_err());
    assert_eq!(page(service, table).await.total, 2);
}

#[allow(clippy::too_many_lines, reason = "one end-to-end flow")]
/// Custom SQL with two statements gives two result sets. Bad SQL is an error.
async fn custom_sql(service: &ConnectionService) {
    let sets = service
        .run_query(ID, "SELECT 1 AS one; SELECT 2 AS two, NULL AS empty_value")
        .await;
    let sets = sets.expect("query");
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].columns, vec!["one"]);
    assert_eq!(sets[1].rows[0], vec![text("2"), None]);

    // Bad SQL is an error, not a panic.
    assert!(service.run_query(ID, "SELEC nonsense").await.is_err());
}

/// Explain gives a plan tree and never runs the statement. More than one statement,
/// or `ANALYZE` in front, is refused. Analyze runs only a read.
async fn query_plan(service: &ConnectionService, table: &TableRef, driver: Driver) {
    let before = page(service, table).await.total;
    let delete = format!("DELETE FROM {TABLE} WHERE id > 0;");
    let plan = service.query_plan(ID, &delete, false).await.expect("plan");
    assert!(!plan.analyzed);
    assert_ne!(plan.nodes.len(), 0);
    assert_ne!(plan.raw.expect("raw").rows.len(), 0);
    assert_eq!(page(service, table).await.total, before);
    // SQL Server turned the plan setting off again, so a query runs for real.
    let sets = service
        .run_query(ID, "SELECT 1 AS one")
        .await
        .expect("query");
    assert_eq!(sets.last().expect("set").rows, vec![vec![text("1")]]);

    let two = format!("SELECT 1; DELETE FROM {TABLE}");
    assert!(service.query_plan(ID, &two, false).await.is_err());
    let analyze = format!("ANALYZE DELETE FROM {TABLE}");
    assert!(service.query_plan(ID, &analyze, false).await.is_err());
    let error = service
        .query_plan(ID, &delete, true)
        .await
        .expect_err("not a read");
    assert_eq!(
        error.to_string(),
        sql_pro_lib::constants::ANALYZE_NOT_A_READ
    );
    assert_eq!(page(service, table).await.total, before);

    let select = format!("SELECT id FROM {TABLE} WHERE id > 0");
    let analyzed = service.query_plan(ID, &select, true).await;
    if driver == Driver::Sqlite {
        let error = analyzed.expect_err("no analyze");
        assert_eq!(error.to_string(), sql_pro_lib::constants::SQLITE_NO_ANALYZE);
        return;
    }
    let plan = analyzed.expect("analyze");
    assert!(plan.analyzed);
    assert_eq!(plan.measure, Some(PlanMeasure::Time));
    assert!(plan.nodes.iter().any(|node| node.time_ms.is_some()));
    // SQL Server turned the statistics off again: one result set, no plan.
    let sets = service
        .run_query(ID, "SELECT 1 AS one")
        .await
        .expect("query");
    assert_eq!(sets.len(), 1);
    if driver == Driver::Mssql {
        plan_setting_stays_private(service).await;
    }
    // A plan that fails leaves the tab working, with no plan setting on.
    let _ = service.query_plan(ID, "SELECT 1/0 AS x", true).await;
    assert!(
        service
            .query_plan(ID, "SELECT * FROM sqlpro_missing", false)
            .await
            .is_err()
    );
    let sets = service
        .run_query(ID, "SELECT 1 AS one")
        .await
        .expect("query after a failed plan");
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].rows, vec![vec![text("1")]]);
}

/// A query of the same tab that comes during an Analyze waits for it. It never runs while
/// `STATISTICS XML` is on, so it gets its own rows and no plan. It reads a table, because
/// SQL Server writes no plan for a trivial query like `SELECT 1`.
async fn plan_setting_stays_private(service: &ConnectionService) {
    // About 0.3 s. The filter keeps SQL Server from counting the rows without reading them.
    let slow = "SELECT COUNT(*) FROM sys.all_columns a CROSS JOIN sys.all_columns b \
        WHERE (a.column_id * b.column_id) % 7 = 3";
    let other = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        service
            .run_query(ID, "SELECT COUNT(*) AS one FROM sys.objects")
            .await
    };
    let started = Instant::now();
    let (plan, sets) = tokio::join!(service.query_plan(ID, slow, true), other);
    assert!(plan.expect("analyze").analyzed);
    let sets = sets.expect("query");
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].columns, vec!["one"]);
    // The check means something only when the other query came while the analyze ran.
    assert!(started.elapsed() > Duration::from_millis(100));
}

/// Every SQL Server type comes back as the text SQL Server itself writes.
/// Types the driver cannot read still show when a table is browsed.
async fn mssql_types(service: &ConnectionService) {
    let sql = "SELECT CAST(1 AS bit), CAST(255 AS tinyint), CAST(-5 AS smallint), \
        CAST(-7 AS int), CAST(9000000000 AS bigint), CAST(12.50 AS decimal(10,2)), \
        CAST(-0.005 AS numeric(5,3)), CAST(-1.5 AS money), CAST(12.34 AS smallmoney), \
        CAST(1.5 AS float), CAST(0.25 AS real), \
        CAST('6F9619FF-8B86-D011-B42D-00C04FC964FF' AS uniqueidentifier), \
        CAST(0x00FF AS varbinary(10)), CAST('2020-01-31' AS date), \
        CAST('13:14:15.5' AS time(3)), CAST('2020-01-31 13:14:15.123' AS datetime), \
        CAST('2020-01-31 13:14:15.1234567' AS datetime2), \
        CAST('2020-01-31 13:14:00' AS smalldatetime), \
        CAST('2020-01-31 13:14:15 +02:00' AS datetimeoffset(0)), CAST('abc' AS char(4)), \
        CAST(N'äö€' AS nvarchar(10)), CAST('café' AS varchar(10)), CAST(NULL AS int)";
    let sets = service.run_query(ID, sql).await.expect("types");
    let expected = [
        "1",
        "255",
        "-5",
        "-7",
        "9000000000",
        "12.50",
        "-0.005",
        "-1.5000",
        "12.3400",
        "1.5",
        "0.25",
        "6F9619FF-8B86-D011-B42D-00C04FC964FF",
        "0x00ff",
        "2020-01-31",
        "13:14:15.500",
        "2020-01-31 13:14:15.123",
        "2020-01-31 13:14:15.1234567",
        "2020-01-31 13:14:00",
        "2020-01-31 13:14:15 +02:00",
        "abc ",
        "äö€",
        "café",
    ];
    let mut wanted: Vec<Option<String>> = expected.iter().map(|value| text(value)).collect();
    wanted.push(None);
    assert_eq!(sets[0].rows[0], wanted);

    let setup = "DROP TABLE IF EXISTS sqlpro_types; \
        CREATE TABLE sqlpro_types (id INT PRIMARY KEY, x XML, t TEXT, n NTEXT, i IMAGE, \
            v SQL_VARIANT, g GEOGRAPHY, h HIERARCHYID, b VARBINARY(10)); \
        INSERT INTO sqlpro_types VALUES (1, '<a>1</a>', 'old', N'new', 0x0A0B, 7, \
            geography::Point(1, 2, 4326), hierarchyid::Parse('/1/'), NULL)";
    service.run_query(ID, setup).await.expect("types table");
    let table = listed(service, "sqlpro_types").await.expect("types table");
    // A binary value is typed as `0x…` text.
    let changes = values(&[("b", Some("0x0102"))]);
    let updated = service.update_row(ID, &table, values(&[("id", Some("1"))]), changes);
    assert_eq!(updated.await.expect("binary update"), 1);
    let row = &page(service, &table).await.rows[0];
    let shown = [
        "1",
        "<a>1</a>",
        "old",
        "new",
        "0x0a0b",
        "7",
        "POINT (2 1)",
        "/1/",
        "0x0102",
    ];
    let shown: Vec<Option<String>> = shown.iter().map(|value| text(value)).collect();
    assert_eq!(*row, shown);
    // The free query editor gives an error for such a type, not a panic.
    let error = service.run_query(ID, "SELECT x FROM sqlpro_types").await;
    let error = error.expect_err("xml").to_string();
    assert!(
        error.contains("type Xml") && error.contains("CAST("),
        "{error}"
    );
    let sets = service
        .run_query(ID, "SELECT 1")
        .await
        .expect("query after it");
    assert_eq!(sets[0].rows[0], vec![text("1")]);
    service
        .run_query(ID, "DROP TABLE sqlpro_types")
        .await
        .expect("drop types table");
}

/// Drops the test table and closes the tab. A closed tab cannot be used.
async fn clean_up(service: &ConnectionService) {
    service
        .run_query(ID, &format!("DROP TABLE {TABLE}"))
        .await
        .expect("cleanup");
    service.disconnect(ID).await;
    assert!(service.tables(ID).await.is_err());
}

/// A slow query stops soon after a cancel, and the tab still works after it.
async fn cancel(service: &ConnectionService, driver: Driver) {
    let slow = match driver {
        Driver::Mysql => "SELECT SLEEP(5)",
        Driver::Postgres => "SELECT pg_sleep(5)",
        Driver::Mssql => "WAITFOR DELAY '00:00:05'",
        // Counts to one billion. That takes far longer than the test waits.
        Driver::Sqlite => {
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 1000000000) \
             SELECT count(*) FROM n"
        }
    };
    let started = Instant::now();
    let later = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        service.cancel_query(ID).await
    };
    let (result, cancelled) = tokio::join!(service.run_query(ID, slow), later);
    cancelled.expect("cancel");
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the query stopped"
    );
    match result {
        Err(error) => assert_eq!(error.to_string(), "Query cancelled"),
        // MySQL ends a cancelled SLEEP without an error. It gives 1.
        Ok(sets) => {
            assert_eq!(driver, Driver::Mysql);
            assert_eq!(sets[0].rows[0], vec![text("1")]);
        }
    }
    let sets = service
        .run_query(ID, "SELECT 1")
        .await
        .expect("query after cancel");
    assert_eq!(sets[0].rows[0], vec![text("1")]);
    // A cancel while nothing runs does nothing.
    service.cancel_query(ID).await.expect("idle cancel");
    let sets = service
        .run_query(ID, "SELECT 2")
        .await
        .expect("query after idle cancel");
    assert_eq!(sets[0].rows[0], vec![text("2")]);
}

/// A database name that must be quoted to work.
const NEW_DATABASE: &str = "sqlpro made \"db\"";

/// Creates and drops a database. `SQLite` has none, so both must fail there.
async fn databases(service: &ConnectionService, config: &ConnectionConfig) {
    let driver = config.driver;
    if driver == Driver::Sqlite {
        assert!(service.create_database(ID, "x", "", "").await.is_err());
        assert!(service.drop_database(ID, "x").await.is_err());
        return;
    }
    // A leftover of an earlier failed run is removed first.
    let _ = service.drop_database(ID, NEW_DATABASE).await;
    let charset = if driver == Driver::Mysql {
        "utf8mb4"
    } else {
        ""
    };
    let created = service.create_database(ID, NEW_DATABASE, charset, "");
    created.await.expect("create database");
    let listed = service.databases(ID).await.expect("databases");
    assert!(listed.iter().any(|name| name == NEW_DATABASE));
    if driver != Driver::Mysql {
        let refused = service
            .create_database(ID, "sqlpro_never", "utf8", "")
            .await;
        assert!(refused.is_err(), "charset is only for MySQL");
    }
    assert!(service.create_database(ID, "  ", "", "").await.is_err());

    // The tab can visit the new database and come back.
    service
        .use_database(ID, NEW_DATABASE.to_owned())
        .await
        .expect("use new database");
    let current = service.drop_database(ID, NEW_DATABASE).await;
    assert!(current.is_err(), "the current database is never dropped");
    service
        .use_database(ID, config.database.clone())
        .await
        .expect("back to the test database");

    service
        .drop_database(ID, NEW_DATABASE)
        .await
        .expect("drop database");
    let listed = service.databases(ID).await.expect("databases");
    assert!(!listed.iter().any(|name| name == NEW_DATABASE));
    assert!(service.drop_database(ID, NEW_DATABASE).await.is_err());
}

/// The id of the process that runs the connection `id`.
async fn own_process(service: &ConnectionService, id: &str) -> i64 {
    let listed = service.list_processes(id).await.expect("list processes");
    let mut current = listed.iter().filter(|process| process.current);
    let own = current.next().expect("the own connection is listed");
    assert!(current.next().is_none(), "only one is marked as current");
    own.id
}

/// Runs a query in the selected database, then finds its shape in the query stats.
/// The list has only the stats of that database.
/// `PostgreSQL` needs `pg_stat_statements` in `shared_preload_libraries`.
async fn query_stats(service: &ConnectionService, config: &ConnectionConfig) {
    let driver = config.driver;
    if driver == Driver::Sqlite {
        let listed = service.query_stats(ID, 7).await;
        assert_eq!(
            listed.expect_err("no stats").to_string(),
            "SQLite has no query stats"
        );
        return;
    }
    if driver == Driver::Postgres {
        service
            .run_query(ID, "CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .await
            .expect("create extension");
    }
    // The list has only the 500 slowest shapes. A test server that ran for long has more,
    // so the cheap probe would not be in it. So the stats start again here.
    let reset = match driver {
        Driver::Mysql => "TRUNCATE TABLE performance_schema.events_statements_summary_by_digest",
        Driver::Postgres => "SELECT pg_stat_statements_reset()",
        _ => "DBCC FREEPROCCACHE",
    };
    service.run_query(ID, reset).await.expect("reset stats");
    let probe = format!("SELECT COUNT(*) AS sqlpro_stats_probe FROM {TABLE}");
    service.run_query(ID, &probe).await.expect("probe");
    // Zero days is out of bounds and becomes one day.
    let stats = service.query_stats(ID, 0).await.expect("query stats");
    assert!(stats.len() <= 500);
    for stat in &stats {
        assert_eq!(
            stat.database.as_deref(),
            Some(config.database.as_str()),
            "{stat:?}"
        );
    }
    let found = stats
        .iter()
        .find(|stat| stat.query.contains("sqlpro_stats_probe"));
    let found = found.unwrap_or_else(|| panic!("the probe is listed: {stats:?}"));
    assert!(found.runs >= 1);
    assert!(found.total_ms >= 0.0);
    assert_eq!(found.last_run.is_some(), driver != Driver::Postgres);
}

/// Lists the processes and stops one. The tab cannot stop its own connection.
/// Leaves `ID` connected.
async fn processes(service: &ConnectionService, config: &ConnectionConfig) {
    if config.driver == Driver::Sqlite {
        let listed = service.list_processes(ID).await;
        assert_eq!(
            listed.expect_err("no processes").to_string(),
            "SQLite has no server processes"
        );
        return;
    }
    let own = own_process(service, ID).await;
    let refused = service.kill_process(ID, own, false).await;
    assert!(
        refused
            .expect_err("own connection")
            .to_string()
            .contains("this tab")
    );
    let gone = service.kill_process(ID, i64::MAX, false).await;
    assert_eq!(
        gone.expect_err("unknown id").to_string(),
        "This process is not running any more"
    );

    let connected = service.connect(CLONE_ID.to_owned(), config.clone()).await;
    connected.expect("connect a second tab");
    let other = own_process(service, CLONE_ID).await;
    assert_ne!(other, own);
    if config.driver == Driver::Mssql {
        assert!(service.kill_process(ID, other, true).await.is_err());
    } else {
        // Stopping only the statement keeps the connection.
        let stopped = service.kill_process(ID, other, true).await;
        stopped.expect("stop the query");
        assert_eq!(own_process(service, CLONE_ID).await, other);
    }
    service
        .kill_process(ID, other, false)
        .await
        .expect("kill the second tab");
    // PostgreSQL ends the process a moment after the signal. SQL Server can be slow
    // while the other tests run.
    let mut ended = false;
    for _ in 0..100 {
        let listed = service.list_processes(ID).await.expect("list processes");
        if listed.iter().all(|process| process.id != other) {
            ended = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ended, "the killed process is gone");
    service.disconnect(CLONE_ID).await;
}

#[allow(clippy::too_many_lines, reason = "one end-to-end flow")]
async fn exercise(config: ConnectionConfig) {
    let driver = config.driver;
    let service = ConnectionService::new(RepositoryFactory);
    service
        .connect(ID.to_owned(), config.clone())
        .await
        .expect("connect");

    service
        .run_query(ID, &format!("DROP TABLE IF EXISTS {TABLE}"))
        .await
        .expect("drop");
    let create = format!(
        "CREATE TABLE {TABLE} (id INTEGER PRIMARY KEY, name VARCHAR(20), born DATE, note TEXT)"
    );
    service.run_query(ID, &create).await.expect("create");

    let mut found = None;
    for table in service.tables(ID).await.expect("tables") {
        if table.name == TABLE {
            found = Some(table);
        }
    }
    let table = found.expect("the new table is listed");
    assert_eq!(table.kind, "table");

    // Insert: values are bound as text and must land in typed columns.
    let first = values(&[
        ("id", Some("1")),
        ("name", Some("O'Brien")),
        ("born", Some("2020-01-31")),
    ]);
    assert_eq!(
        service
            .insert_row(ID, &table, first)
            .await
            .expect("insert 1"),
        1
    );
    let second = values(&[("id", Some("2")), ("note", Some("line 1\nline 2"))]);
    assert_eq!(
        service
            .insert_row(ID, &table, second)
            .await
            .expect("insert 2"),
        1
    );

    let loaded = page(&service, &table).await;
    assert_eq!(loaded.total, 2);
    paging(&service, &table).await;
    assert_eq!(loaded.columns.len(), 4);
    assert!(loaded.columns[0].primary_key);
    assert!(!loaded.columns[1].primary_key);
    assert_eq!(
        loaded.rows[0],
        vec![text("1"), text("O'Brien"), text("2020-01-31"), None]
    );
    assert_eq!(
        loaded.rows[1],
        vec![text("2"), None, None, text("line 1\nline 2")]
    );

    dump(&service, driver, &config.database).await;
    export(&service, &table).await;
    insert_statements(&service, driver);
    import(&service, &table).await;
    filters(&service, &table).await;
    save_changes(&service, &table).await;
    clone_connection(&service, &config, &table).await;
    processes(&service, &config).await;
    query_stats(&service, &config).await;
    users(&service, &config, &table).await;

    // Update: only the row with the given key changes.
    let changes = values(&[("name", Some("Ann")), ("note", None)]);
    let updated = service
        .update_row(ID, &table, values(&[("id", Some("2"))]), changes)
        .await;
    assert_eq!(updated.expect("update"), 1);

    // An update without the key must fail. It must never touch all rows.
    let changes = values(&[("name", Some("everyone"))]);
    assert!(
        service
            .update_row(ID, &table, Values::new(), changes)
            .await
            .is_err()
    );
    // A column that is not in the table must fail too.
    let changes = values(&[("nope", Some("x"))]);
    assert!(
        service
            .update_row(ID, &table, values(&[("id", Some("2"))]), changes)
            .await
            .is_err()
    );

    delete_row(&service, &table, "1").await;
    let loaded = page(&service, &table).await;
    assert_eq!(loaded.total, 1);
    assert_eq!(loaded.rows[0], vec![text("2"), text("Ann"), None, None]);

    schema_changes(&service, &table, driver).await;
    drop_and_truncate(&service).await;
    objects(&service, driver).await;
    save_objects(&service, driver).await;
    create_and_rename(&service).await;
    diagram(&service, driver).await;
    keys(&service, driver).await;
    keys_together(&service, driver).await;
    cancel(&service, driver).await;
    databases(&service, &config).await;
    save_image();

    custom_sql(&service).await;
    query_plan(&service, &table, driver).await;
    if driver == Driver::Mssql {
        mssql_types(&service).await;
    }

    clean_up(&service).await;
    // Here, not in a test of its own: run at the same time, its sequence on
    // `PostgreSQL` can come and go while `pg_dump` reads the database.
    if driver != Driver::Sqlite {
        ai_read_guard(config).await;
    }
}

#[tokio::test]
async fn sqlite() {
    let path = std::env::temp_dir().join(format!("sqlpro-check-{}.sqlite", std::process::id()));
    std::fs::File::create(&path).expect("temp file");
    exercise(ConnectionConfig {
        driver: Driver::Sqlite,
        host: String::new(),
        port: 0,
        user: String::new(),
        password: String::new(),
        database: path.to_string_lossy().into_owned(),
        ssh: None,
        ssl: sql_pro_lib::model::SslConfig::default(),
    })
    .await;
    std::fs::remove_file(&path).expect("remove temp file");
}

#[tokio::test]
async fn mysql() {
    if let Some(config) = config_from_env("SQLPRO_TEST_MYSQL", Driver::Mysql) {
        exercise(config).await;
    }
}

#[tokio::test]
async fn mariadb() {
    if let Some(config) = config_from_env("SQLPRO_TEST_MARIADB", Driver::Mysql) {
        exercise(config).await;
    }
}

#[tokio::test]
async fn postgres() {
    if let Some(config) = config_from_env("SQLPRO_TEST_POSTGRES", Driver::Postgres) {
        exercise(config).await;
    }
}

/// The SQL Server image has no setting for a first database, so the test makes it.
#[tokio::test]
async fn mssql() {
    let Some(config) = config_from_env("SQLPRO_TEST_MSSQL", Driver::Mssql) else {
        return;
    };
    let mut master = config.clone();
    "master".clone_into(&mut master.database);
    let service = ConnectionService::new(RepositoryFactory);
    // A wrong password fails, and the error never shows it.
    let mut wrong = master.clone();
    "Wrong-Pass-9@/x".clone_into(&mut wrong.password);
    let error = service.connect(ID.to_owned(), wrong).await;
    let error = error.expect_err("wrong password").to_string();
    assert!(!error.contains("Wrong-Pass"), "{error}");
    service
        .connect(ID.to_owned(), master)
        .await
        .expect("connect to master");
    let literal = config.database.replace('\'', "''");
    let name = config.database.replace(']', "]]");
    let create = format!("IF DB_ID(N'{literal}') IS NULL CREATE DATABASE [{name}]");
    service.run_query(ID, &create).await.expect("test database");
    service.disconnect(ID).await;
    exercise(config).await;
}

/// The guard of AI reads: a read runs, an open transaction blocks it, and nothing stays open.
/// On `PostgreSQL` it also blocks `nextval`, a write that looks like a read.
async fn ai_read_guard(config: ConnectionConfig) {
    let driver = config.driver;
    let service = ConnectionService::new(RepositoryFactory);
    let id = "ai-guard";
    service
        .connect(id.to_owned(), config)
        .await
        .expect("connect");
    let sets = service
        .run_ai_read(id, "SELECT 1 AS one;")
        .await
        .expect("read");
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].rows, vec![vec![Some("1".to_owned())]]);

    let begin = if driver == Driver::Mssql {
        "BEGIN TRAN"
    } else {
        "BEGIN"
    };
    service.run_query(id, begin).await.expect("begin");
    let error = service
        .run_ai_read(id, "SELECT 1")
        .await
        .expect_err("open transaction");
    assert_eq!(error.to_string(), sql_pro_lib::constants::OPEN_TRANSACTION);
    // Analyze runs in the same guard, so it waits for the end of the transaction too.
    let error = service
        .query_plan(id, "SELECT 1", true)
        .await
        .expect_err("open transaction");
    assert_eq!(error.to_string(), sql_pro_lib::constants::OPEN_TRANSACTION);
    // A failed Explain keeps the connection, so the transaction is still open.
    assert!(
        service
            .query_plan(id, "SELECT * FROM sqlpro_missing", false)
            .await
            .is_err()
    );
    assert!(service.run_ai_read(id, "SELECT 1").await.is_err());
    service.run_query(id, "ROLLBACK").await.expect("rollback");
    service
        .run_ai_read(id, "SELECT 1")
        .await
        .expect("read after rollback");
    // A read that fails inside the guard leaves the tab working.
    let _ = service.run_ai_read(id, "SELECT 1/0 AS x").await;
    let sets = service
        .run_ai_read(id, "SELECT 1 AS one")
        .await
        .expect("read after a failed read");
    assert_eq!(sets[0].rows, vec![vec![Some("1".to_owned())]]);
    if driver == Driver::Mssql {
        compile_error_keeps_the_session(&service, id).await;
    }

    if driver == Driver::Mysql {
        service
            .run_query(id, "SET autocommit = 0")
            .await
            .expect("autocommit off");
        assert!(service.run_ai_read(id, "SELECT 1").await.is_err());
        service
            .run_query(id, "SET autocommit = 1")
            .await
            .expect("autocommit on");
    }
    if driver == Driver::Postgres {
        service
            .run_query(
                id,
                "DROP SEQUENCE IF EXISTS sqlpro_ai_seq; CREATE SEQUENCE sqlpro_ai_seq",
            )
            .await
            .expect("sequence");
        let error = service
            .run_ai_read(id, "SELECT nextval('sqlpro_ai_seq')")
            .await;
        assert!(
            error
                .expect_err("read-only")
                .to_string()
                .contains("read-only")
        );
        // Analyze runs the read in the guard too, so the write of `nextval` fails.
        let error = service
            .query_plan(id, "SELECT nextval('sqlpro_ai_seq')", true)
            .await;
        assert!(
            error
                .expect_err("read-only")
                .to_string()
                .contains("read-only")
        );
        // The failed read left no transaction behind.
        service
            .run_ai_read(id, "SELECT 1")
            .await
            .expect("read after error");
        service
            .run_query(id, "DROP SEQUENCE sqlpro_ai_seq")
            .await
            .expect("drop");
    }
    service.disconnect(id).await;
}

/// On SQL Server a compile error stops the whole guarded batch before `BEGIN TRAN` runs.
/// The tab keeps its connection then, so a `#temp` table made before is still there.
async fn compile_error_keeps_the_session(service: &ConnectionService, id: &str) {
    service
        .run_query(id, "CREATE TABLE #sqlpro_kept (a int)")
        .await
        .expect("temp table");
    assert!(service.run_ai_read(id, "SELECT (1").await.is_err());
    // A table that is there: a missing table is found only at run time.
    let missing_column = "SELECT sqlpro_missing FROM sys.objects";
    assert!(service.run_ai_read(id, missing_column).await.is_err());
    assert!(service.query_plan(id, missing_column, true).await.is_err());
    service
        .run_query(id, "SELECT COUNT(*) FROM #sqlpro_kept")
        .await
        .expect("the temp table is still there");
    service
        .run_query(id, "DROP TABLE #sqlpro_kept")
        .await
        .expect("drop temp table");
}

/// A cancel stops a connect that hangs. Then no connection is kept for the tab.
#[tokio::test]
async fn cancel_connect_keeps_nothing() {
    // This "server" takes the TCP connection but never answers, so the connect hangs.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let config = ConnectionConfig {
        driver: Driver::Mysql,
        host: "127.0.0.1".to_owned(),
        port: listener.local_addr().expect("address").port(),
        user: "root".to_owned(),
        password: String::new(),
        database: String::new(),
        ssh: None,
        ssl: sql_pro_lib::model::SslConfig::default(),
    };
    let service = ConnectionService::new(RepositoryFactory);
    let started = Instant::now();
    let (result, ()) = tokio::join!(service.connect(ID.to_owned(), config), async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        service.cancel_connect(ID);
    });
    let error = result.expect_err("the connect was cancelled");
    assert_eq!(error.to_string(), sql_pro_lib::constants::CONNECT_CANCELLED);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(
        service.databases(ID).await.is_err(),
        "no connection is kept"
    );
    // A cancel when no connect runs does nothing.
    service.cancel_connect(ID);
}

/// The two tabs of the schema compare test.
const SYNC_SOURCE: &str = "sync-source";
const SYNC_TARGET: &str = "sync-target";

/// Runs SQL statements in a tab, one by one.
async fn run_all(service: &ConnectionService, id: &str, statements: &[&str]) {
    for sql in statements {
        service.run_query(id, sql).await.expect(sql);
    }
}

/// Makes the target like the source with schema compare, in two rounds: first only the
/// new table, then all the rest. In the end the compare finds no difference.
/// `SQLite` cannot change a column, so there the shared column is the same on both sides.
/// `PostgreSQL` also copies a `serial` table with a `UNIQUE` column and an index on an
/// expression. That index is copied from the statement `pg_get_indexdef` gives. Its
/// generated column with a changed expression cannot be changed, so it stays with a warning.
/// SQL Server copies an index with `DESC`, `INCLUDE` and `WHERE`.
#[allow(clippy::too_many_lines, reason = "one end-to-end flow")]
async fn schema_sync(service: &ConnectionService, driver: Driver) {
    let sqlite = driver == Driver::Sqlite;
    let postgres = driver == Driver::Postgres;
    let mysql = driver == Driver::Mysql;
    let sql_server = driver == Driver::Mssql;
    let mut source = vec![
        "CREATE TABLE sync_parent (id INTEGER PRIMARY KEY, name VARCHAR(40) NOT NULL, \
            note VARCHAR(10) DEFAULT 'it''s')",
        "CREATE INDEX sync_parent_name ON sync_parent (name)",
        "CREATE TABLE sync_child (id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL, \
            FOREIGN KEY (parent_id) REFERENCES sync_parent (id))",
    ];
    if postgres {
        source.push("CREATE TABLE sync_serial (id SERIAL PRIMARY KEY, email VARCHAR(40) UNIQUE)");
        source.push("CREATE INDEX sync_serial_lower ON sync_serial (lower(email))");
        source.push(
            "CREATE TABLE sync_gen (id INTEGER PRIMARY KEY, a INTEGER, \
                total INTEGER GENERATED ALWAYS AS (a * 2) STORED)",
        );
    }
    if sql_server {
        source.push(
            "CREATE INDEX sync_parent_inc ON sync_parent (name DESC) INCLUDE (note) \
                WHERE note IS NOT NULL",
        );
    }
    // MySQL and MariaDB: an expression default, a text default that looks like a number
    // word, bit and binary defaults, a time default that is only added to a column, and
    // indexes with a prefix, DESC and FULLTEXT.
    if mysql {
        source.push(
            "CREATE TABLE sync_extra (id INTEGER PRIMARY KEY, body TEXT, \
                code VARCHAR(36) DEFAULT (concat('a', 'b')), label VARCHAR(10) DEFAULT 'inf', \
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP, flag BIT(1) DEFAULT b'1', \
                raw VARBINARY(4) DEFAULT 'ab')",
        );
        source.push("CREATE INDEX sync_extra_body ON sync_extra (body(20), label DESC)");
        source.push("CREATE FULLTEXT INDEX sync_extra_text ON sync_extra (body)");
    }
    run_all(service, SYNC_SOURCE, &source).await;
    if mysql {
        run_all(
            service,
            SYNC_TARGET,
            &["CREATE TABLE sync_extra (id INTEGER PRIMARY KEY, body TEXT, created_at DATETIME)"],
        )
        .await;
    }
    if postgres {
        run_all(
            service,
            SYNC_TARGET,
            &["CREATE TABLE sync_gen (id INTEGER PRIMARY KEY, a INTEGER, \
                total INTEGER GENERATED ALWAYS AS (a * 3) STORED)"],
        )
        .await;
    }
    // SQLite cannot drop a column with a UNIQUE constraint. The others drop the
    // constraint first. It owns an index, which `DROP INDEX` cannot drop.
    let parent = if sqlite {
        "CREATE TABLE sync_parent (id INTEGER PRIMARY KEY, name VARCHAR(40) NOT NULL, \
            old_col INTEGER)"
    } else {
        "CREATE TABLE sync_parent (id INTEGER PRIMARY KEY, name VARCHAR(20), \
            old_col INTEGER, CONSTRAINT sync_parent_old_key UNIQUE (old_col))"
    };
    run_all(
        service,
        SYNC_TARGET,
        &[
            parent,
            "CREATE INDEX sync_parent_old ON sync_parent (old_col)",
            "CREATE TABLE sync_gone (id INTEGER PRIMARY KEY)",
        ],
    )
    .await;

    let changes = service
        .compare_schemas(SYNC_SOURCE, SYNC_TARGET)
        .await
        .expect("compare");
    let mut found = Vec::new();
    for change in &changes {
        found.push(format!(
            "{:?} {} {}",
            change.action, change.table.name, change.name
        ));
    }
    let mut expected = vec![
        "CreateTable sync_child ",
        "DropTable sync_gone ",
        "AddColumn sync_parent note",
        "DropColumn sync_parent old_col",
        "AddIndex sync_parent sync_parent_name",
        "DropIndex sync_parent sync_parent_old",
    ];
    if !sqlite {
        expected.insert(2, "AlterColumn sync_parent name");
        expected.push("DropIndex sync_parent sync_parent_old_key");
    }
    if postgres {
        expected.insert(1, "AlterColumn sync_gen total");
        expected.push("CreateTable sync_serial ");
    }
    if sql_server {
        expected.insert(5, "AddIndex sync_parent sync_parent_inc");
    }
    if mysql {
        let extra = [
            "AddColumn sync_extra code",
            "AddColumn sync_extra label",
            "AlterColumn sync_extra created_at",
            "AddColumn sync_extra flag",
            "AddColumn sync_extra raw",
            "AddIndex sync_extra sync_extra_body",
            "AddIndex sync_extra sync_extra_text",
        ];
        expected.splice(1..1, extra);
    }
    assert_eq!(found, expected);

    // Round one: only the new table. The SQL is only made here, so nothing changed yet.
    let create: Vec<String> = changes
        .iter()
        .filter(|change| change.table.name == "sync_child")
        .map(|change| change.id.clone())
        .collect();
    let sql = service
        .schema_sync_sql(SYNC_SOURCE, SYNC_TARGET, &create)
        .await
        .expect("sync sql");
    assert!(sql.warnings.is_empty(), "{:?}", sql.warnings);
    assert_ne!(sql.statements.len(), 0);
    let again = service.compare_schemas(SYNC_SOURCE, SYNC_TARGET).await;
    assert_eq!(again.expect("compare").len(), changes.len());

    // SQL that is not what the backend builds now never runs.
    let other = ["DROP TABLE sync_parent".to_owned()];
    let refused = service
        .run_schema_sync(
            SYNC_SOURCE,
            SYNC_TARGET,
            &create,
            &other,
            sql.database.as_deref(),
        )
        .await;
    assert_eq!(
        refused.expect_err("other sql").to_string(),
        sql_pro_lib::constants::SYNC_SQL_CHANGED
    );
    let tables = service.tables(SYNC_TARGET).await.expect("tables");
    assert!(tables.iter().any(|table| table.name == "sync_parent"));

    service
        .run_schema_sync(
            SYNC_SOURCE,
            SYNC_TARGET,
            &create,
            &sql.statements,
            sql.database.as_deref(),
        )
        .await
        .expect("run round one");
    let rest = service
        .compare_schemas(SYNC_SOURCE, SYNC_TARGET)
        .await
        .expect("compare");
    assert_eq!(rest.len(), changes.len() - 1);
    assert!(rest.iter().all(|change| change.table.name != "sync_child"));

    // Round two: everything that is left.
    let ids: Vec<String> = rest.iter().map(|change| change.id.clone()).collect();
    let sql = service
        .schema_sync_sql(SYNC_SOURCE, SYNC_TARGET, &ids)
        .await
        .expect("sync sql");
    let warned = if postgres {
        vec![format!(
            "Change column total of public.sync_gen: {}",
            sql_pro_lib::constants::COLUMN_RULE_CHANGED
        )]
    } else {
        Vec::new()
    };
    assert_eq!(sql.warnings, warned);
    service
        .run_schema_sync(
            SYNC_SOURCE,
            SYNC_TARGET,
            &ids,
            &sql.statements,
            sql.database.as_deref(),
        )
        .await
        .expect("run round two");
    let left = service
        .compare_schemas(SYNC_SOURCE, SYNC_TARGET)
        .await
        .expect("compare");
    let mut found = Vec::new();
    for change in &left {
        found.push(format!("{:?} {}", change.action, change.name));
    }
    let wanted: Vec<&str> = if postgres {
        vec!["AlterColumn total"]
    } else {
        Vec::new()
    };
    assert_eq!(found, wanted);
    if postgres {
        // The serial column works: a new row gets its number.
        run_all(
            service,
            SYNC_TARGET,
            &["INSERT INTO sync_serial (email) VALUES ('a@b.c')"],
        )
        .await;
    }
}

#[tokio::test]
async fn sqlite_schema_sync() {
    let service = ConnectionService::new(RepositoryFactory);
    let mut paths = Vec::new();
    for id in [SYNC_SOURCE, SYNC_TARGET] {
        let path = temp_path(id, "sqlite");
        std::fs::File::create(&path).expect("temp file");
        let config = ConnectionConfig {
            driver: Driver::Sqlite,
            host: String::new(),
            port: 0,
            user: String::new(),
            password: String::new(),
            database: path.to_string_lossy().into_owned(),
            ssh: None,
            ssl: sql_pro_lib::model::SslConfig::default(),
        };
        service
            .connect(id.to_owned(), config)
            .await
            .expect("connect");
        paths.push(path);
    }
    schema_sync(&service, Driver::Sqlite).await;
    for (id, path) in [SYNC_SOURCE, SYNC_TARGET].iter().zip(&paths) {
        service.disconnect(id).await;
        std::fs::remove_file(path).expect("remove temp file");
    }
}

/// Opens tab `main` on the server and two tabs `ids` on two new databases, so the other
/// tests that run at the same time do not get in the way.
async fn server_pair(
    config: &ConnectionConfig,
    main: &str,
    ids: [&str; 2],
    databases: [&str; 2],
) -> ConnectionService {
    let service = ConnectionService::new(RepositoryFactory);
    service
        .connect(main.to_owned(), config.clone())
        .await
        .expect("connect");
    for (id, database) in ids.iter().zip(databases) {
        // A leftover of an earlier failed run is removed first.
        let _ = service.drop_database(main, database).await;
        let created = service.create_database(main, database, "", "").await;
        created.expect("create database");
        let mut tab = config.clone();
        database.clone_into(&mut tab.database);
        service
            .connect((*id).to_owned(), tab)
            .await
            .expect("connect");
    }
    service
}

/// Closes the tabs of `server_pair` and drops its two databases.
async fn drop_pair(service: &ConnectionService, main: &str, ids: [&str; 2], databases: [&str; 2]) {
    for id in ids {
        service.disconnect(id).await;
    }
    for database in databases {
        service
            .drop_database(main, database)
            .await
            .expect("drop database");
    }
    service.disconnect(main).await;
}

/// Schema compare on a server: the source and the target are two new databases.
async fn server_schema_sync(config: ConnectionConfig) {
    const MAIN: &str = "sync-main";
    const DATABASES: [&str; 2] = ["sqlpro_sync_source", "sqlpro_sync_target"];
    let service = server_pair(&config, MAIN, [SYNC_SOURCE, SYNC_TARGET], DATABASES).await;

    // Two kinds of database cannot be compared.
    let path = temp_path("sync-other", "sqlite");
    std::fs::File::create(&path).expect("temp file");
    let sqlite = ConnectionConfig {
        driver: Driver::Sqlite,
        host: String::new(),
        port: 0,
        user: String::new(),
        password: String::new(),
        database: path.to_string_lossy().into_owned(),
        ssh: None,
        ssl: sql_pro_lib::model::SslConfig::default(),
    };
    service
        .connect("sync-other".to_owned(), sqlite)
        .await
        .expect("connect");
    let mixed = service.compare_schemas(SYNC_SOURCE, "sync-other").await;
    assert!(
        mixed
            .expect_err("two kinds")
            .to_string()
            .contains("same kind")
    );
    service.disconnect("sync-other").await;
    std::fs::remove_file(&path).ok();

    schema_sync(&service, config.driver).await;
    drop_pair(&service, MAIN, [SYNC_SOURCE, SYNC_TARGET], DATABASES).await;
}

#[tokio::test]
async fn mysql_schema_sync() {
    if let Some(config) = config_from_env("SQLPRO_TEST_MYSQL", Driver::Mysql) {
        server_schema_sync(config).await;
    }
}

#[tokio::test]
async fn mariadb_schema_sync() {
    if let Some(config) = config_from_env("SQLPRO_TEST_MARIADB", Driver::Mysql) {
        server_schema_sync(config).await;
    }
}

#[tokio::test]
async fn postgres_schema_sync() {
    if let Some(config) = config_from_env("SQLPRO_TEST_POSTGRES", Driver::Postgres) {
        server_schema_sync(config).await;
    }
}

#[tokio::test]
async fn mssql_schema_sync() {
    // The test database may not be there yet. `master` always is.
    if let Some(mut config) = config_from_env("SQLPRO_TEST_MSSQL", Driver::Mssql) {
        "master".clone_into(&mut config.database);
        server_schema_sync(config).await;
    }
}

/// The two tabs of the data compare test.
const DATA_SOURCE: &str = "data-source";
const DATA_TARGET: &str = "data-target";

/// The table `name` of tab `id`, with the schema the database gives it.
async fn find_table(service: &ConnectionService, id: &str, name: &str) -> TableRef {
    let tables = service.tables(id).await.expect("tables");
    tables
        .into_iter()
        .find(|table| table.name == name)
        .expect("table is there")
}

/// Rows `ids` of `table` as one `INSERT` each 500 rows. SQL Server takes at most 1000.
fn bulk_rows(table: &str, ids: std::ops::Range<u32>, changed: u32) -> Vec<String> {
    let mut statements = Vec::new();
    let all: Vec<u32> = ids.collect();
    for chunk in all.chunks(500) {
        let mut rows = Vec::new();
        for id in chunk {
            let name = if *id == changed { "changed" } else { "same" };
            rows.push(format!("({id}, '{name}', NULL)"));
        }
        statements.push(format!(
            "INSERT INTO {table} (id, name, note) VALUES {}",
            rows.join(", ")
        ));
    }
    statements
}

/// Makes the target rows like the source rows with data compare. Then a compare finds
/// no difference. The rows cross the page edge, a key has two columns, and an identity
/// column gets the value of the source.
#[allow(clippy::too_many_lines, reason = "one end-to-end flow")]
async fn data_sync(service: &ConnectionService, driver: Driver) {
    let sqlite = driver == Driver::Sqlite;
    let identity = match driver {
        Driver::Postgres => "INTEGER GENERATED ALWAYS AS IDENTITY",
        Driver::Mssql => "INTEGER IDENTITY(1,1)",
        Driver::Mysql => "INTEGER AUTO_INCREMENT",
        Driver::Sqlite => "INTEGER",
    };
    let auto = format!("CREATE TABLE data_auto (id {identity} PRIMARY KEY, name VARCHAR(10))");
    for (id, extra) in [(DATA_SOURCE, "only_source"), (DATA_TARGET, "only_target")] {
        let item = format!(
            "CREATE TABLE data_item (id INTEGER PRIMARY KEY, name VARCHAR(40), \
                note VARCHAR(40), {extra} INTEGER)"
        );
        run_all(
            service,
            id,
            &[
                &item,
                "CREATE TABLE data_pair (a INTEGER NOT NULL, b VARCHAR(10) NOT NULL, \
                    v VARCHAR(10), PRIMARY KEY (a, b))",
                "CREATE TABLE data_loose (x INTEGER)",
                &auto,
            ],
        )
        .await;
    }
    let mut source = vec![
        "INSERT INTO data_item (id, name, note) VALUES (1, 'a', NULL), (2, 'b', ''), \
            (3, 'it''s', 'x'), (4, 'new', NULL)"
            .to_owned(),
        "INSERT INTO data_pair (a, b, v) VALUES (1, 'x', 'one'), (1, 'y', 'two')".to_owned(),
        "INSERT INTO data_auto (name) VALUES ('a')".to_owned(),
        "INSERT INTO data_auto (name) VALUES ('b')".to_owned(),
    ];
    source.extend(bulk_rows("data_item", 1000..3100, 0));
    let mut target = vec![
        "INSERT INTO data_item (id, name, note) VALUES (1, 'a', NULL), (2, 'b', NULL), \
            (3, 'old', 'x'), (5, 'gone', NULL)"
            .to_owned(),
        "INSERT INTO data_pair (a, b, v) VALUES (1, 'x', 'ONE'), (2, 'y', 'two')".to_owned(),
        "INSERT INTO data_auto (name) VALUES ('a')".to_owned(),
    ];
    // One changed row on the second page.
    target.extend(bulk_rows("data_item", 1000..3100, 2999));
    let source: Vec<&str> = source.iter().map(String::as_str).collect();
    let target: Vec<&str> = target.iter().map(String::as_str).collect();
    run_all(service, DATA_SOURCE, &source).await;
    run_all(service, DATA_TARGET, &target).await;

    // A table without a primary key cannot be compared.
    let loose = find_table(service, DATA_SOURCE, "data_loose").await;
    let refused = service.compare_data(DATA_SOURCE, DATA_TARGET, &loose).await;
    assert!(
        refused
            .expect_err("no key")
            .to_string()
            .contains("no primary key")
    );

    let all = DataKinds {
        insert: true,
        update: true,
        delete: true,
    };
    let item = find_table(service, DATA_SOURCE, "data_item").await;
    let diff = service
        .compare_data(DATA_SOURCE, DATA_TARGET, &item)
        .await
        .expect("compare");
    assert_eq!(diff.key, ["id"]);
    assert_eq!(diff.columns, ["id", "name", "note"]);
    assert_eq!(diff.source_only, ["only_source"]);
    assert_eq!(diff.target_only, ["only_target"]);
    assert!(!diff.limited);
    assert_eq!(
        (diff.delete_rows, diff.update_rows, diff.insert_rows),
        (1, 3, 1)
    );
    if sqlite {
        assert_eq!(diff.deletes, [r#"DELETE FROM "data_item" WHERE "id" = 5"#]);
        assert_eq!(
            diff.updates,
            [
                r#"UPDATE "data_item" SET "note" = '' WHERE "id" = 2"#,
                r#"UPDATE "data_item" SET "name" = 'it''s' WHERE "id" = 3"#,
                r#"UPDATE "data_item" SET "name" = 'same' WHERE "id" = 2999"#,
            ]
        );
        assert_eq!(
            diff.inserts,
            [r#"INSERT INTO "data_item" ("id", "name", "note") VALUES (4, 'new', NULL)"#]
        );
    }

    // SQL that is not what the backend builds now never runs.
    let other = ["DELETE FROM data_item".to_owned()];
    let refused = service
        .run_data_sync(
            DATA_SOURCE,
            DATA_TARGET,
            &item,
            all,
            &other,
            diff.database.as_deref(),
        )
        .await;
    assert_eq!(
        refused.expect_err("other sql").to_string(),
        sql_pro_lib::constants::SYNC_SQL_CHANGED
    );

    // Round one: only the updates.
    let updates = DataKinds {
        insert: false,
        delete: false,
        ..all
    };
    service
        .run_data_sync(
            DATA_SOURCE,
            DATA_TARGET,
            &item,
            updates,
            &diff.updates,
            diff.database.as_deref(),
        )
        .await
        .expect("run updates");
    let diff = service
        .compare_data(DATA_SOURCE, DATA_TARGET, &item)
        .await
        .expect("compare");
    assert_eq!(
        (diff.delete_rows, diff.update_rows, diff.insert_rows),
        (1, 0, 1)
    );

    // Round two: the rest, for every table.
    for name in ["data_item", "data_pair", "data_auto"] {
        sync_table(service, name).await;
    }
    data_types(service, driver).await;
}

/// Compares table `name`, runs all of its SQL, and checks that a new compare finds
/// nothing. Gives the first compare.
async fn sync_table(service: &ConnectionService, name: &str) -> sql_pro_lib::model::DataDiff {
    let all = DataKinds {
        insert: true,
        update: true,
        delete: true,
    };
    let table = find_table(service, DATA_SOURCE, name).await;
    let diff = service
        .compare_data(DATA_SOURCE, DATA_TARGET, &table)
        .await
        .expect("compare");
    let mut reviewed = diff.deletes.clone();
    reviewed.extend(diff.updates.clone());
    reviewed.extend(diff.inserts.clone());
    assert_ne!(reviewed.len(), 0, "{name}");
    service
        .run_data_sync(
            DATA_SOURCE,
            DATA_TARGET,
            &table,
            all,
            &reviewed,
            diff.database.as_deref(),
        )
        .await
        .expect(name);
    let left = service
        .compare_data(DATA_SOURCE, DATA_TARGET, &table)
        .await
        .expect("compare");
    assert_eq!(
        (left.delete_rows, left.update_rows, left.insert_rows),
        (0, 0, 0),
        "{name}"
    );
    diff
}

/// The first cell of the first row of `sql` on the target tab.
async fn target_cell(service: &ConnectionService, sql: &str) -> Option<String> {
    let sets = service.run_query(DATA_TARGET, sql).await.expect(sql);
    sets.into_iter()
        .next()?
        .rows
        .into_iter()
        .next()?
        .into_iter()
        .next()?
}

/// Makes table `make` on both tabs and fills it with `source` and `target`.
async fn data_table(service: &ConnectionService, make: &str, source: &str, target: &str) {
    run_all(service, DATA_SOURCE, &[make, source]).await;
    run_all(service, DATA_TARGET, &[make, target]).await;
}

/// A `varchar` key sorts with the SQL collation of the server. The page after a key must
/// use the same order, or rows are skipped and look like rows to delete.
async fn mssql_varchar_keys(service: &ConnectionService) {
    let mut keys: Vec<String> = (1..2000).map(|id| format!("a{id:04}")).collect();
    // The source has exactly one full page up to `b-z`. An `nvarchar` compare puts `b-z`
    // after `ba` and `bb`, so a page after `b-z` would miss them.
    for key in ["b-z", "ba", "bb"] {
        keys.push(key.to_owned());
    }
    let make = "CREATE TABLE data_text (k VARCHAR(10) COLLATE SQL_Latin1_General_CP1_CI_AS PRIMARY KEY, v INTEGER)";
    for (id, extra) in [(DATA_SOURCE, ""), (DATA_TARGET, "a0000")] {
        let mut all = keys.clone();
        if !extra.is_empty() {
            all.push(extra.to_owned());
        }
        let mut statements = vec![make.to_owned()];
        for chunk in all.chunks(500) {
            let rows: Vec<String> = chunk.iter().map(|key| format!("('{key}', 1)")).collect();
            statements.push(format!("INSERT INTO data_text VALUES {}", rows.join(", ")));
        }
        let statements: Vec<&str> = statements.iter().map(String::as_str).collect();
        run_all(service, id, &statements).await;
    }
    let diff = sync_table(service, "data_text").await;
    assert_eq!(
        diff.deletes,
        ["DELETE FROM [dbo].[data_text] WHERE [k] = \
            CAST(N'a0000' COLLATE SQL_Latin1_General_CP1_CI_AS AS varchar(10))"]
    );
    assert_eq!((diff.update_rows, diff.insert_rows), (0, 0));
}

/// SQL Server keys that a plain cast breaks: a `varchar` with another code page than
/// the database, an alias type, and a `datetime` in a session with a day-month language.
async fn mssql_key_types(service: &ConnectionService) {
    data_table(
        service,
        "CREATE TABLE data_cn (k VARCHAR(10) COLLATE Chinese_PRC_CI_AS PRIMARY KEY, v INTEGER)",
        "INSERT INTO data_cn VALUES (N'中文', 1), (N'乙', 2)",
        "INSERT INTO data_cn VALUES (N'中文', 9), (N'甲', 1)",
    )
    .await;
    let diff = sync_table(service, "data_cn").await;
    assert_eq!(
        (diff.delete_rows, diff.update_rows, diff.insert_rows),
        (1, 1, 1)
    );
    let text = target_cell(
        service,
        "SELECT STRING_AGG(CONCAT(CAST(k AS NVARCHAR(10)), ':', v), ',') WITHIN GROUP (ORDER BY v) \
            FROM data_cn",
    )
    .await;
    assert_eq!(text.as_deref(), Some("中文:1,乙:2"));

    for id in [DATA_SOURCE, DATA_TARGET] {
        run_all(service, id, &["CREATE TYPE dbo.CodeT FROM VARCHAR(20)"]).await;
    }
    data_table(
        service,
        "CREATE TABLE data_alias (k dbo.CodeT PRIMARY KEY, v INTEGER)",
        "INSERT INTO data_alias VALUES ('a', 1)",
        "INSERT INTO data_alias VALUES ('a', 2), ('b', 1)",
    )
    .await;
    // The alias type is cast to its base type. `CodeT` is no system type.
    let diff = sync_table(service, "data_alias").await;
    assert_eq!(diff.deletes.len(), 1);
    assert!(
        diff.deletes[0].ends_with(" AS varchar(20))"),
        "{}",
        diff.deletes[0]
    );

    data_table(
        service,
        "CREATE TABLE data_time (k DATETIME PRIMARY KEY, v INTEGER)",
        "INSERT INTO data_time VALUES ('2024-01-13T10:00:00.007', 1)",
        "INSERT INTO data_time VALUES ('2024-01-13T10:00:00.007', 2), ('2024-01-14T10:00:00', 1)",
    )
    .await;
    // `British` reads `2024-01-13` as year, day, month: day 2024-01, month 13 fails.
    run_all(service, DATA_TARGET, &["SET LANGUAGE British"]).await;
    sync_table(service, "data_time").await;

    // Values too: British reads `2024-01-05` as the 1st of May, with no error.
    data_table(
        service,
        "CREATE TABLE data_when (id INTEGER PRIMARY KEY, at DATETIME, s SMALLDATETIME)",
        "INSERT INTO data_when VALUES (1, '2024-01-05T10:00:00', '2024-02-03T08:00:00'), \
            (2, '2024-03-04T00:00:00.007', NULL)",
        "INSERT INTO data_when VALUES (1, '2024-12-31T00:00:00', NULL)",
    )
    .await;
    sync_table(service, "data_when").await;
    let stored = target_cell(
        service,
        "SELECT STRING_AGG(CONCAT(id, '|', CONVERT(VARCHAR(23), at, 121), '|', \
            ISNULL(CONVERT(VARCHAR(16), s, 121), 'null')), ',') WITHIN GROUP (ORDER BY id) \
            FROM data_when",
    )
    .await;
    assert_eq!(
        stored.as_deref(),
        Some("1|2024-01-05 10:00:00.000|2024-02-03 08:00,2|2024-03-04 00:00:00.007|null")
    );
    run_all(service, DATA_TARGET, &["SET LANGUAGE us_english"]).await;

    // An alias type over `varchar` is read as text of its code page: `€` and curly quotes.
    for id in [DATA_SOURCE, DATA_TARGET] {
        run_all(service, id, &["CREATE TYPE dbo.NameT FROM VARCHAR(20)"]).await;
    }
    data_table(
        service,
        "CREATE TABLE data_name (k dbo.NameT PRIMARY KEY, v dbo.NameT)",
        "INSERT INTO data_name VALUES (N'€ key', N'price 5 €'), (N'“q”', N'‘x’')",
        "INSERT INTO data_name VALUES (N'€ key', N'old'), (N'gone €', N'x')",
    )
    .await;
    let diff = sync_table(service, "data_name").await;
    assert_eq!(
        (diff.delete_rows, diff.update_rows, diff.insert_rows),
        (1, 1, 1)
    );
    let names = target_cell(
        service,
        "SELECT STRING_AGG(CONCAT(CAST(k AS NVARCHAR(20)), '=', CAST(v AS NVARCHAR(20))), ',') \
            WITHIN GROUP (ORDER BY v) FROM data_name",
    )
    .await;
    assert_eq!(names.as_deref(), Some("“q”=‘x’,€ key=price 5 €"));
}

/// A `hierarchyid` key over more than one page. Its text order (`/1/`, `/10/`, ...) is
/// not its real order, so the read must sort by the column, not by the text.
async fn mssql_tree_pages(service: &ConnectionService) {
    let make = "CREATE TABLE data_pages (h HIERARCHYID PRIMARY KEY, v INT)";
    for (id, changed) in [(DATA_SOURCE, 0), (DATA_TARGET, 2001)] {
        let mut statements = vec![make.to_owned()];
        let ids: Vec<u32> = (1..=2001).collect();
        for chunk in ids.chunks(500) {
            let rows: Vec<String> = chunk
                .iter()
                .map(|key| {
                    format!(
                        "('/{key}/', {})",
                        u8::from(*key == changed || *key == 5 && changed > 0)
                    )
                })
                .collect();
            statements.push(format!("INSERT INTO data_pages VALUES {}", rows.join(", ")));
        }
        let statements: Vec<&str> = statements.iter().map(String::as_str).collect();
        run_all(service, id, &statements).await;
    }
    let diff = sync_table(service, "data_pages").await;
    assert_eq!(
        (diff.delete_rows, diff.update_rows, diff.insert_rows),
        (0, 2, 0)
    );

    // A sql_variant key cannot be matched.
    data_table(
        service,
        "CREATE TABLE data_variant (s SQL_VARIANT PRIMARY KEY, v INT)",
        "INSERT INTO data_variant VALUES (5, 1)",
        "SELECT 1",
    )
    .await;
    let table = find_table(service, DATA_SOURCE, "data_variant").await;
    let refused = service.compare_data(DATA_SOURCE, DATA_TARGET, &table).await;
    assert!(
        refused
            .expect_err("sql_variant key")
            .to_string()
            .contains("sql_variant")
    );
}

/// CLR types, `xml` and `sql_variant` are read as text and written back as text.
/// A `hierarchyid` key is cast to its own type.
async fn mssql_clr_types(service: &ConnectionService) {
    data_table(
        service,
        "CREATE TABLE data_clr (id INT PRIMARY KEY, g GEOGRAPHY, h HIERARCHYID, x XML, \
            s SQL_VARIANT)",
        "INSERT INTO data_clr VALUES (1, geography::Point(1, 2, 4326), '/1/', '<a>1</a>', 5)",
        "INSERT INTO data_clr VALUES (1, geography::Point(3, 4, 4326), '/2/', '<a>2</a>', 5)",
    )
    .await;
    let diff = sync_table(service, "data_clr").await;
    assert_eq!(diff.updates.len(), 1);
    for column in ["[g]", "[h]", "[x]"] {
        assert!(diff.updates[0].contains(column), "{}", diff.updates[0]);
    }
    let values = target_cell(
        service,
        "SELECT CONCAT(g.ToString(), '|', h.ToString(), '|', CAST(x AS NVARCHAR(20))) FROM data_clr",
    )
    .await;
    assert_eq!(values.as_deref(), Some("POINT (2 1)|/1/|<a>1</a>"));
    data_table(
        service,
        "CREATE TABLE data_tree (h HIERARCHYID PRIMARY KEY, v INT)",
        "INSERT INTO data_tree VALUES ('/1/', 1)",
        "INSERT INTO data_tree VALUES ('/1/', 2), ('/2/', 1)",
    )
    .await;
    let diff = sync_table(service, "data_tree").await;
    assert_eq!(
        diff.deletes,
        ["DELETE FROM [dbo].[data_tree] WHERE [h] = CAST(N'/2/' AS hierarchyid)"]
    );
}

/// Values that only some databases have trouble with: binary bytes that are not UTF-8,
/// columns the database fills itself, columns with no type (`SQLite`), and NULL keys.
async fn data_types(service: &ConnectionService, driver: Driver) {
    let (bytes, wanted) = match driver {
        Driver::Sqlite => {
            // No declared type: a quoted '1' would never match the number 1.
            data_table(
                service,
                "CREATE TABLE data_bin (id PRIMARY KEY, v)",
                "INSERT INTO data_bin VALUES (1, 10), (2, 'two'), (3, X'FF00'), (5, NULL)",
                "INSERT INTO data_bin VALUES (1, 11), (4, 5), (5, '')",
            )
            .await;
            let diff = sync_table(service, "data_bin").await;
            assert_eq!(
                (diff.delete_rows, diff.update_rows, diff.insert_rows),
                (1, 2, 2)
            );
            // A text key may be NULL in SQLite. Such a row cannot be matched.
            data_table(
                service,
                "CREATE TABLE data_nk (id TEXT PRIMARY KEY, v INTEGER)",
                "INSERT INTO data_nk VALUES (NULL, 1)",
                "SELECT 1",
            )
            .await;
            let table = find_table(service, DATA_SOURCE, "data_nk").await;
            let refused = service.compare_data(DATA_SOURCE, DATA_TARGET, &table).await;
            assert!(
                refused
                    .expect_err("null key")
                    .to_string()
                    .contains("NULL in its primary key")
            );
            (
                "SELECT group_concat(id || ':' || typeof(id) || ':' || typeof(v) || ':' || \
                    coalesce(hex(v), 'null'), ',') FROM (SELECT * FROM data_bin ORDER BY id)",
                "1:integer:integer:3130,2:integer:text:74776F,3:integer:blob:FF00,\
                    5:integer:null:",
            )
        }
        Driver::Mysql => {
            data_table(
                service,
                "CREATE TABLE data_bin (id INTEGER PRIMARY KEY, b VARBINARY(10), l BLOB, \
                    bits BIT(8))",
                "INSERT INTO data_bin VALUES (1, 0xFF00, 0x00FE, b'11001000'), \
                    (2, X'', 'text', b'1'), (3, NULL, NULL, NULL)",
                "INSERT INTO data_bin VALUES (1, 0xFF01, NULL, b'0')",
            )
            .await;
            sync_table(service, "data_bin").await;
            (
                "SELECT GROUP_CONCAT(CONCAT(id, ':', IFNULL(HEX(b), 'null'), ':', \
                    IFNULL(HEX(l), 'null'), ':', IFNULL(bits + 0, 'null')) ORDER BY id) \
                    FROM data_bin",
                "1:FF00:00FE:200,2::74657874:1,3:null:null:null",
            )
        }
        Driver::Mssql => {
            data_table(
                service,
                "CREATE TABLE data_bin (id INTEGER PRIMARY KEY, b VARBINARY(10), \
                    rv ROWVERSION, n INTEGER IDENTITY(1,1), name VARCHAR(10))",
                "INSERT INTO data_bin (id, b, name) VALUES (1, 0xFF00, 'a'), (2, 0x, 'b'), \
                    (3, NULL, 'c')",
                "INSERT INTO data_bin (id, b, name) VALUES (9, NULL, 'x'), (1, 0xFF01, 'z')",
            )
            .await;
            let diff = sync_table(service, "data_bin").await;
            assert_eq!(diff.generated, ["rv", "n"]);
            mssql_varchar_keys(service).await;
            mssql_key_types(service).await;
            mssql_clr_types(service).await;
            mssql_tree_pages(service).await;
            (
                "SELECT STRING_AGG(CONCAT(id, ':', ISNULL(CONVERT(VARCHAR(30), b, 1), 'null'), \
                    ':', name), ',') WITHIN GROUP (ORDER BY id) FROM data_bin",
                "1:0xFF00:a,2:0x:b,3:null:c",
            )
        }
        Driver::Postgres => {
            data_table(
                service,
                "CREATE TABLE data_bin (id INTEGER PRIMARY KEY, b BYTEA, \
                    n INTEGER GENERATED ALWAYS AS IDENTITY, name VARCHAR(10))",
                "INSERT INTO data_bin (id, b, name) VALUES (1, '\\xff00', 'a'), (2, NULL, 'b')",
                "INSERT INTO data_bin (id, b, name) VALUES (9, NULL, 'x'), (1, '\\xff01', 'z')",
            )
            .await;
            let diff = sync_table(service, "data_bin").await;
            assert_eq!(diff.generated, ["n"]);
            (
                "SELECT string_agg(id || ':' || coalesce(encode(b, 'hex'), 'null') || ':' || \
                    name, ',' ORDER BY id) FROM data_bin",
                "1:ff00:a,2:null:b",
            )
        }
    };
    assert_eq!(target_cell(service, bytes).await.as_deref(), Some(wanted));
}

#[tokio::test]
async fn sqlite_data_sync() {
    let service = ConnectionService::new(RepositoryFactory);
    let mut paths = Vec::new();
    for id in [DATA_SOURCE, DATA_TARGET] {
        let path = temp_path(id, "sqlite");
        std::fs::File::create(&path).expect("temp file");
        let config = ConnectionConfig {
            driver: Driver::Sqlite,
            host: String::new(),
            port: 0,
            user: String::new(),
            password: String::new(),
            database: path.to_string_lossy().into_owned(),
            ssh: None,
            ssl: sql_pro_lib::model::SslConfig::default(),
        };
        service
            .connect(id.to_owned(), config)
            .await
            .expect("connect");
        paths.push(path);
    }
    data_sync(&service, Driver::Sqlite).await;
    for (id, path) in [DATA_SOURCE, DATA_TARGET].iter().zip(&paths) {
        service.disconnect(id).await;
        std::fs::remove_file(path).expect("remove temp file");
    }
}

/// Data compare on a server, on two new databases.
async fn server_data_sync(config: ConnectionConfig) {
    const MAIN: &str = "data-main";
    const DATABASES: [&str; 2] = ["sqlpro_data_source", "sqlpro_data_target"];
    let ids = [DATA_SOURCE, DATA_TARGET];
    let service = server_pair(&config, MAIN, ids, DATABASES).await;
    data_sync(&service, config.driver).await;
    drop_pair(&service, MAIN, ids, DATABASES).await;
}

#[tokio::test]
async fn mysql_data_sync() {
    if let Some(config) = config_from_env("SQLPRO_TEST_MYSQL", Driver::Mysql) {
        server_data_sync(config).await;
    }
}

#[tokio::test]
async fn mariadb_data_sync() {
    if let Some(config) = config_from_env("SQLPRO_TEST_MARIADB", Driver::Mysql) {
        server_data_sync(config).await;
    }
}

#[tokio::test]
async fn postgres_data_sync() {
    if let Some(config) = config_from_env("SQLPRO_TEST_POSTGRES", Driver::Postgres) {
        server_data_sync(config).await;
    }
}

#[tokio::test]
async fn mssql_data_sync() {
    // The test database may not be there yet. `master` always is.
    if let Some(mut config) = config_from_env("SQLPRO_TEST_MSSQL", Driver::Mssql) {
        "master".clone_into(&mut config.database);
        server_data_sync(config).await;
    }
}
