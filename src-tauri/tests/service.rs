//! One end-to-end check of the service against real databases.
//!
//! `SQLite` always runs. `MySQL` and `PostgreSQL` run only when these are set:
//! `SQLPRO_TEST_MYSQL=host:port:user:password:database`
//! `SQLPRO_TEST_POSTGRES=host:port:user:password:database`
//! Add `SQLPRO_TEST_SSH=host:port:user:password` to run both through an SSH tunnel.

use sql_pro_lib::model::{
    BrowseRequest, ColumnSpec, ConnectionConfig, Driver, FilterOperator, FilterRule, SshConfig,
    TableRef, Values,
};
use sql_pro_lib::repository::RepositoryFactory;
use sql_pro_lib::service::ConnectionService;

const ID: &str = "test";
const TABLE: &str = "sqlpro_check";
const EXTRA: &str = "sqlpro_extra";
const EXTRA_VIEW: &str = "sqlpro_extra_view";

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

/// Set `SQLPRO_TEST_SSH=host:port:user:password` to run MySQL and PostgreSQL through a tunnel.
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
    }
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

    service
        .delete_rows(ID, table, vec![values(&[("id", Some("3"))])])
        .await
        .expect("delete 3");
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

async fn listed(service: &ConnectionService, name: &str) -> Option<TableRef> {
    let tables = service.tables(ID).await.expect("tables");
    tables.into_iter().find(|table| table.name == name)
}

/// Truncates and drops an extra table, and a view on it.
async fn drop_and_truncate(service: &ConnectionService) {
    let setup = format!(
        "DROP VIEW IF EXISTS {EXTRA_VIEW}; DROP TABLE IF EXISTS {EXTRA}; \
        CREATE TABLE {EXTRA} (id INTEGER PRIMARY KEY); \
        INSERT INTO {EXTRA} (id) VALUES (1); INSERT INTO {EXTRA} (id) VALUES (2); \
        CREATE VIEW {EXTRA_VIEW} AS SELECT id FROM {EXTRA}"
    );
    service.run_query(ID, &setup).await.expect("extra table");
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

/// Dumps the database to a file. Expects the test table with the name O'Brien in it.
async fn dump(service: &ConnectionService, driver: Driver) {
    let folder = std::env::temp_dir();
    let path = folder.join(format!("sqlpro-dump-{}-{driver:?}.sql", std::process::id()));
    match service.dump_database(ID, &path).await {
        Err(error) if error.to_string().contains("was not found") => {
            println!("NOTE: the dump check for {driver:?} is skipped: {error}");
            return;
        }
        result => result.expect("dump"),
    }
    let sql = std::fs::read_to_string(&path).expect("dump file");
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

#[allow(clippy::too_many_lines, reason = "one end-to-end flow")]
async fn exercise(config: ConnectionConfig) {
    let driver = config.driver;
    let service = ConnectionService::new(RepositoryFactory);
    service
        .connect(ID.to_owned(), config)
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

    dump(&service, driver).await;
    filters(&service, &table).await;

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

    assert_eq!(
        service
            .delete_rows(ID, &table, vec![values(&[("id", Some("1"))])])
            .await
            .expect("delete"),
        1
    );
    let loaded = page(&service, &table).await;
    assert_eq!(loaded.total, 1);
    assert_eq!(loaded.rows[0], vec![text("2"), text("Ann"), None, None]);

    schema_changes(&service, &table, driver).await;
    drop_and_truncate(&service).await;

    // Custom SQL with two statements gives two result sets.
    let sets = service
        .run_query(ID, "SELECT 1 AS one; SELECT 2 AS two, NULL AS empty_value")
        .await;
    let sets = sets.expect("query");
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].columns, vec!["one"]);
    assert_eq!(sets[1].rows[0], vec![text("2"), None]);

    // Bad SQL is an error, not a panic.
    assert!(service.run_query(ID, "SELEC nonsense").await.is_err());

    service
        .run_query(ID, &format!("DROP TABLE {TABLE}"))
        .await
        .expect("cleanup");
    service.disconnect(ID).await;
    assert!(service.tables(ID).await.is_err());
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
async fn postgres() {
    if let Some(config) = config_from_env("SQLPRO_TEST_POSTGRES", Driver::Postgres) {
        exercise(config).await;
    }
}
