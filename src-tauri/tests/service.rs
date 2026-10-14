//! One end-to-end check of the service against real databases.
//!
//! `SQLite` always runs. `MySQL` and `PostgreSQL` run only when these are set:
//! `SQLPRO_TEST_MYSQL=host:port:user:password:database`
//! `SQLPRO_TEST_POSTGRES=host:port:user:password:database`
//! Add `SQLPRO_TEST_SSH=host:port:user:password` to run both through an SSH tunnel.

use sql_pro_lib::model::{
    AccessLevel, BrowseRequest, ColumnSpec, ConnectionConfig, DbUser, Driver, FilterOperator,
    FilterRule, NewColumn, NewUser, RowUpdate, SaveResult, SshConfig, TableRef, Values,
};
use sql_pro_lib::repository::RepositoryFactory;
use sql_pro_lib::service::ConnectionService;

const ID: &str = "test";
const TABLE: &str = "sqlpro_check";
const EXTRA: &str = "sqlpro_extra";
const EXTRA_VIEW: &str = "sqlpro_extra_view";
const MADE: &str = "sqlpro_made";
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

/// `SQLite` has no accounts: an empty list, and every change is an error.
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
    if driver == Driver::Sqlite {
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
    save_changes(&service, &table).await;
    clone_connection(&service, &config, &table).await;
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
    create_and_rename(&service).await;

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
