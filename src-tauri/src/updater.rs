//! Auto update. When the app starts, it looks for a newer version.
//! If there is one, it asks the user, then downloads, installs and restarts.
//!
//! Release builds only. A debug build never checks and never updates.

use constants::{UPDATE_ERROR_TITLE, UPDATE_INSTALL_TEXT, UPDATE_LATER_TEXT, UPDATE_TITLE};
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt as _, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt as _};

use crate::constants;

/// Starts the check in the background. The app does not wait for it.
///
/// ponytail: the check runs only once, at start. Closing the window hides the
/// app, so an app that runs for days does not check again. Add a timer when
/// that matters.
pub fn start(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(check(app.clone()));
}

/// Looks for a newer version and asks the user when there is one.
///
/// A check that fails stays silent: the user may be offline, or there is no
/// release yet. That is not worth a popup.
async fn check(app: AppHandle) {
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => Err(error),
    };
    let update = match found {
        Ok(Some(update)) => update,
        Ok(None) => return,
        Err(error) => {
            eprintln!("SQL Pro: update check failed: {error}");
            return;
        }
    };
    let question = question(&update.version, &update.current_version);
    // `show` does not block. The answer comes back on another thread.
    app.dialog()
        .message(question)
        .title(UPDATE_TITLE)
        .buttons(MessageDialogButtons::OkCancelCustom(
            UPDATE_INSTALL_TEXT.into(),
            UPDATE_LATER_TEXT.into(),
        ))
        .show(move |yes| {
            if yes {
                tauri::async_runtime::spawn(install(app, update));
            }
        });
}

/// The text of the question. `new` is the version on offer, `current` the one that runs.
fn question(new: &str, current: &str) -> String {
    format!(
        "SQL Pro {new} is available. You have {current}. Install it now? \
         The app will restart. Changes you did not save will be lost."
    )
}

/// Downloads and installs the update, then starts the app again.
///
/// This can fail. On Linux only the `AppImage` can update itself: an app
/// installed from a .deb or .rpm file fails here. Then the user sees why.
async fn install(app: AppHandle, update: Update) {
    // ponytail: no progress bar. Add one when the download gets big.
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => app.restart(),
        Err(error) => app
            .dialog()
            .message(format!("The update could not be installed.\n\n{error}"))
            .title(UPDATE_ERROR_TITLE)
            .kind(MessageDialogKind::Error)
            .show(|_| {}),
    }
}

#[cfg(test)]
mod tests {
    use super::question;

    #[test]
    fn the_question_names_both_versions() {
        let text = question("0.2.0", "0.1.0");
        assert!(text.starts_with("SQL Pro 0.2.0 is available. You have 0.1.0."));
    }
}
