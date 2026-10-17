//! SQL Pro backend. Layers: commands -> service -> repository -> model.

mod commands;
pub mod constants;
pub mod error;
pub mod model;
mod open_file;
pub mod repository;
pub mod service;
mod updater;

use std::sync::Arc;

use constants::{CLAUDE_FOLDER, MAIN_WINDOW, PROFILE, PROFILE_GLOBAL, PROFILE_PLUGIN};
use repository::{OsKeychain, PasswordRepository, RepositoryFactory};
use service::ConnectionService;
use tauri::Manager as _;

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
    // This plugin must be the first one. A debug build leaves it out, so a dev
    // build and the installed app can run side by side.
    #[cfg(all(not(target_os = "macos"), not(debug_assertions)))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
        show_main_window(app);
        // The first argument is the program itself.
        let files = open_file::file_args(args.into_iter().skip(1), std::path::Path::new(&cwd));
        open_file::receive(app, files);
    }));
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu);
    let app = builder
        .setup(setup)
        .on_window_event(hide_instead_of_close)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(profile_plugin())
        .manage(ConnectionService::new(RepositoryFactory))
        .manage(PasswordRepository::new(Arc::new(OsKeychain)))
        .manage(open_file::OpenedFiles::default())
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::test_connection,
            commands::disconnect,
            commands::clone_connection,
            commands::use_database,
            commands::list_databases,
            commands::create_database,
            commands::drop_database,
            commands::list_tables,
            commands::browse,
            commands::run_query,
            commands::cancel_query,
            commands::ask_claude,
            commands::run_ai_read,
            commands::is_read_sql,
            commands::read_attachment,
            commands::attachment_from_base64,
            commands::parse_csv,
            commands::stop_claude,
            commands::update_row,
            commands::insert_row,
            commands::save_changes,
            commands::add_column,
            commands::drop_column,
            commands::alter_column,
            commands::table_keys,
            commands::add_index,
            commands::drop_index,
            commands::add_foreign_key,
            commands::drop_foreign_key,
            commands::create_table,
            commands::rename_table,
            commands::drop_table,
            commands::truncate_table,
            commands::list_objects,
            commands::object_definition,
            commands::drop_object,
            commands::dump_database,
            commands::export_tables,
            commands::export_rows,
            commands::list_columns,
            commands::preview_csv,
            commands::import_csv,
            commands::import_sql,
            commands::diagram,
            commands::save_image,
            commands::read_text_file,
            commands::write_text_file,
            commands::list_users,
            commands::create_user,
            commands::drop_user,
            commands::set_user_access,
            commands::set_user_password,
            commands::list_processes,
            commands::kill_process,
            commands::save_password,
            commands::load_password,
            commands::delete_password,
            commands::take_opened_files,
        ])
        .build(tauri::generate_context!())
        .expect("error while running SQL Pro");
    app.run(on_run_event);
}

/// Closing the main window only hides it. The app keeps running, with all tabs,
/// connections and unsaved edits as they are.
///
/// Quit still quits: Cmd-Q and the app menu on macOS, the tray menu elsewhere.
/// Those do not send a close request, so they do not pass through here.
fn hide_instead_of_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    if window.label() != MAIN_WINDOW {
        return;
    }
    // Without a tray icon the user could not get a hidden window back.
    // Then the window closes as before, and the app quits.
    #[cfg(not(target_os = "macos"))]
    if window.app_handle().tray_by_id(constants::TRAY_ID).is_none() {
        return;
    }
    // A window that cannot be hidden closes as before. So it never gets stuck.
    if window.hide().is_ok() {
        api.prevent_close();
    }
}

/// Shows the main window again and gives it the keyboard.
pub(crate) fn show_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    // These only fail when the window is gone. Then there is nothing to show.
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// macOS: a click on the Dock icon brings the hidden window back.
/// A file opened from the Finder comes in here too.
#[cfg(target_os = "macos")]
#[allow(clippy::needless_pass_by_value)] // Tauri hands the event over by value.
fn on_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    match event {
        tauri::RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } => show_main_window(app),
        tauri::RunEvent::Opened { urls } => open_file::receive(app, open_file::file_urls(&urls)),
        _ => {}
    }
}

/// Windows and Linux have no Dock. The tray icon brings the window back.
#[cfg(not(target_os = "macos"))]
#[allow(clippy::needless_pass_by_value)] // Tauri hands the event over by value.
fn on_run_event(_app: &tauri::AppHandle, _event: tauri::RunEvent) {}

/// Runs once when the app starts. Tauri keeps only one `setup` function,
/// so all start work goes in here.
#[allow(clippy::unnecessary_wraps)] // Tauri wants this return type for `setup`.
fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(target_os = "macos"))]
    add_tray(app);
    updater::start(app.handle());
    // Windows and Linux start the app with the file to open as an argument.
    if let Ok(cwd) = std::env::current_dir() {
        let files = open_file::file_args(std::env::args_os().skip(1), &cwd);
        open_file::receive(app.handle(), files);
    }
    // Claude finds an old session by the folder it runs in. The temp folder gets cleaned.
    if let Ok(data) = app.path().app_data_dir() {
        app.state::<ConnectionService>()
            .set_claude_folder(data.join(CLAUDE_FOLDER));
    }
    Ok(())
}

/// Adds the tray icon with its menu: show the window, or quit for real.
///
/// A tray that cannot be made does not stop the app. Closing the window then
/// quits the app, as it did before.
#[cfg(not(target_os = "macos"))]
fn add_tray(app: &tauri::App) {
    if let Err(error) = build_tray(app) {
        eprintln!("SQL Pro: no tray icon: {error}");
    }
}

#[cfg(not(target_os = "macos"))]
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use constants::{TRAY_ID, TRAY_QUIT_ID, TRAY_QUIT_TEXT, TRAY_SHOW_ID, TRAY_SHOW_TEXT};
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItem::with_id(app, TRAY_SHOW_ID, TRAY_SHOW_TEXT, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT_ID, TRAY_QUIT_TEXT, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(&app.package_info().name)
        .menu(&menu)
        // A left click shows the window. The menu opens on a right click.
        // Linux sends no click, so there the menu is the only way.
        .show_menu_on_left_click(false)
        .on_menu_event(on_tray_menu)
        .on_tray_icon_event(on_tray_click);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::needless_pass_by_value)] // Tauri hands the event over by value.
fn on_tray_menu(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    match event.id.as_ref() {
        constants::TRAY_SHOW_ID => show_main_window(app),
        constants::TRAY_QUIT_ID => app.exit(0),
        _ => {}
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::needless_pass_by_value)] // Tauri hands the event over by value.
fn on_tray_click(tray: &tauri::tray::TrayIcon, event: tauri::tray::TrayIconEvent) {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        show_main_window(tray.app_handle());
    }
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
