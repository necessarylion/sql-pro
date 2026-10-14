//! SQL Pro backend. Layers: commands -> service -> repository -> model.

mod commands;
pub mod constants;
pub mod error;
pub mod model;
pub mod repository;
pub mod service;

use std::sync::Arc;

use constants::{PROFILE, PROFILE_GLOBAL, PROFILE_PLUGIN};
use repository::{OsKeychain, PasswordRepository, RepositoryFactory};
use service::ConnectionService;

/// Starts the app. All objects are built here and handed to the commands as state.
///
/// # Panics
///
/// Panics when the window cannot be created.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(all(debug_assertions, target_os = "macos"))]
    run_under_dev_name();
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu);
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(profile_plugin())
        .manage(ConnectionService::new(RepositoryFactory))
        .manage(PasswordRepository::new(Arc::new(OsKeychain)))
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::disconnect,
            commands::clone_connection,
            commands::use_database,
            commands::list_databases,
            commands::list_tables,
            commands::browse,
            commands::run_query,
            commands::update_row,
            commands::insert_row,
            commands::save_changes,
            commands::add_column,
            commands::drop_column,
            commands::alter_column,
            commands::create_table,
            commands::rename_table,
            commands::drop_table,
            commands::truncate_table,
            commands::dump_database,
            commands::list_users,
            commands::create_user,
            commands::drop_user,
            commands::set_user_access,
            commands::set_user_password,
            commands::save_password,
            commands::load_password,
            commands::delete_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SQL Pro");
}

/// Tells the frontend which build it runs in, before any script of the page runs.
///
/// The frontend reads `window.__SQL_PRO_PROFILE__` when it starts and keeps its
/// saved data apart per profile. It cannot ask with a command: that answer comes too late.
fn profile_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let script = format!("window.{PROFILE_GLOBAL} = {PROFILE:?};");
    tauri::plugin::Builder::new(PROFILE_PLUGIN)
        .js_init_script(script)
        .build()
}

/// Starts the program again under the file name "SQL Pro". Dev builds on macOS only.
///
/// macOS names a program without an app bundle after its file: in the Dock, in
/// Cmd-Tab and in the menu bar. That file is `sql-pro`, and Cargo cannot build a
/// file name with a space. So we make a hard link with the right name next to
/// the program and run that. A bundled app gets its name from the bundle, so
/// nothing happens there.
#[cfg(all(debug_assertions, target_os = "macos"))]
fn run_under_dev_name() {
    use std::os::unix::process::CommandExt as _;

    use constants::DEV_APP_NAME;

    let Ok(program) = std::env::current_exe() else {
        return;
    };
    let bundled = program.to_string_lossy().contains(".app/Contents/");
    if bundled || program.file_name().is_some_and(|name| name == DEV_APP_NAME) {
        return;
    }
    let link = program.with_file_name(DEV_APP_NAME);
    // An old link points at the build before this one.
    let _ = std::fs::remove_file(&link);
    if std::fs::hard_link(&program, &link).is_ok() {
        // `exec` keeps the process id, so `tauri dev` can still stop the app.
        // It only comes back when it failed. Then we go on under the old name.
        let _error = std::process::Command::new(&link)
            .args(std::env::args_os().skip(1))
            .exec();
    }
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
