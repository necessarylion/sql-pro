//! SQL Pro backend. Layers: commands -> service -> repository -> model.

mod commands;
pub mod constants;
pub mod error;
pub mod model;
pub mod repository;
pub mod service;

use repository::{PasswordRepository, RepositoryFactory};
use service::ConnectionService;

/// Starts the app. All objects are built here and handed to the commands as state.
///
/// # Panics
///
/// Panics when the window cannot be created.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu);
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(ConnectionService::new(RepositoryFactory))
        .manage(PasswordRepository)
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::disconnect,
            commands::use_database,
            commands::list_databases,
            commands::list_tables,
            commands::browse,
            commands::run_query,
            commands::update_row,
            commands::insert_row,
            commands::delete_rows,
            commands::add_column,
            commands::drop_column,
            commands::alter_column,
            commands::drop_table,
            commands::truncate_table,
            commands::dump_database,
            commands::save_password,
            commands::load_password,
            commands::delete_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SQL Pro");
}

/// The default menu, with the app name fixed.
///
/// macOS names "About", "Hide" and "Quit" after the program file. Without an app
/// bundle (`tauri dev`) that is the Cargo package name, with a dash.
#[cfg(target_os = "macos")]
fn app_menu(handle: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItemKind};

    let menu = Menu::default(handle)?;
    let name = &handle.package_info().name;
    if let Some(MenuItemKind::Submenu(app_items)) = menu.items()?.first() {
        for item in app_items.items()? {
            if let MenuItemKind::Predefined(item) = item {
                let text = item.text()?;
                if text.contains(env!("CARGO_PKG_NAME")) {
                    item.set_text(text.replace(env!("CARGO_PKG_NAME"), name))?;
                }
            }
        }
    }
    Ok(menu)
}
