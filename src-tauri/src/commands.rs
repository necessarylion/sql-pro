//! Tauri commands. They only pass calls from the frontend on to the service.

// Tauri hands state and arguments over by value.
#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use tauri::State;

use crate::error::AppResult;
use crate::model::{
    BrowseRequest, ColumnSpec, ConnectionConfig, ResultSet, TablePage, TableRef, Values,
};
use crate::repository::PasswordRepository;
use crate::service::ConnectionService;

type Service<'state> = State<'state, ConnectionService>;
type Passwords<'state> = State<'state, PasswordRepository>;

#[tauri::command]
pub async fn connect(service: Service<'_>, id: String, config: ConnectionConfig) -> AppResult<()> {
    service.connect(id, config).await
}

#[tauri::command]
pub async fn disconnect(service: Service<'_>, id: String) -> AppResult<()> {
    service.disconnect(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn use_database(service: Service<'_>, id: String, database: String) -> AppResult<()> {
    service.use_database(&id, database).await
}

#[tauri::command]
pub async fn list_databases(service: Service<'_>, id: String) -> AppResult<Vec<String>> {
    service.databases(&id).await
}

#[tauri::command]
pub async fn list_tables(service: Service<'_>, id: String) -> AppResult<Vec<TableRef>> {
    service.tables(&id).await
}

#[tauri::command]
pub async fn browse(
    service: Service<'_>,
    id: String,
    request: BrowseRequest,
) -> AppResult<TablePage> {
    service.browse(&id, &request).await
}

#[tauri::command]
pub async fn run_query(service: Service<'_>, id: String, sql: String) -> AppResult<Vec<ResultSet>> {
    service.run_query(&id, &sql).await
}

#[tauri::command]
pub async fn update_row(
    service: Service<'_>,
    id: String,
    table: TableRef,
    key: Values,
    changes: Values,
) -> AppResult<u64> {
    service.update_row(&id, &table, key, changes).await
}

#[tauri::command]
pub async fn insert_row(
    service: Service<'_>,
    id: String,
    table: TableRef,
    values: Values,
) -> AppResult<u64> {
    service.insert_row(&id, &table, values).await
}

#[tauri::command]
pub async fn delete_rows(
    service: Service<'_>,
    id: String,
    table: TableRef,
    keys: Vec<Values>,
) -> AppResult<u64> {
    service.delete_rows(&id, &table, keys).await
}

#[tauri::command]
pub async fn add_column(
    service: Service<'_>,
    id: String,
    table: TableRef,
    column: ColumnSpec,
) -> AppResult<()> {
    service.add_column(&id, &table, &column).await
}

#[tauri::command]
pub async fn drop_column(
    service: Service<'_>,
    id: String,
    table: TableRef,
    name: String,
) -> AppResult<()> {
    service.drop_column(&id, &table, &name).await
}

#[tauri::command]
pub async fn alter_column(
    service: Service<'_>,
    id: String,
    table: TableRef,
    name: String,
    column: ColumnSpec,
) -> AppResult<()> {
    service.alter_column(&id, &table, &name, &column).await
}

#[tauri::command]
pub async fn drop_table(service: Service<'_>, id: String, table: TableRef) -> AppResult<()> {
    service.drop_table(&id, &table).await
}

#[tauri::command]
pub async fn truncate_table(service: Service<'_>, id: String, table: TableRef) -> AppResult<()> {
    service.truncate_table(&id, &table).await
}

#[tauri::command]
pub async fn dump_database(service: Service<'_>, id: String, path: PathBuf) -> AppResult<()> {
    service.dump_database(&id, &path).await
}

// The password commands are async so a keychain prompt does not freeze the window.

#[tauri::command]
pub async fn save_password(
    passwords: Passwords<'_>,
    id: String,
    password: String,
) -> AppResult<()> {
    passwords.save(&id, &password)
}

#[tauri::command]
pub async fn load_password(passwords: Passwords<'_>, id: String) -> AppResult<String> {
    passwords.load(&id)
}

#[tauri::command]
pub async fn delete_password(passwords: Passwords<'_>, id: String) -> AppResult<()> {
    passwords.delete(&id)
}
