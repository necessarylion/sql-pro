//! SQLite files that the OS asks the app to open ("Open With" or a double click).
//!
//! The paths can come before the page has loaded. So they wait in `OpenedFiles`
//! until the frontend takes them. An event tells a page that is already running.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use tauri::{Emitter as _, Manager as _};

use crate::constants::OPEN_FILES_EVENT;

/// The paths that the frontend has not taken yet.
#[derive(Default)]
pub struct OpenedFiles(Mutex<Vec<PathBuf>>);

impl OpenedFiles {
    /// Gives all waiting paths and forgets them, so each file opens once.
    pub fn take(&self) -> Vec<PathBuf> {
        std::mem::take(&mut *self.0.lock())
    }
}

/// The files among program arguments. Other arguments (flags, folders, missing files) are dropped.
/// A relative path is read from `cwd`.
pub fn file_args(args: impl IntoIterator<Item = impl Into<OsString>>, cwd: &Path) -> Vec<PathBuf> {
    args.into_iter()
        .map(|arg| cwd.join(arg.into()))
        .filter(|path| path.is_file())
        .collect()
}

/// The files among `file://` URLs. macOS sends these.
#[cfg(target_os = "macos")]
pub fn file_urls(urls: &[tauri::Url]) -> Vec<PathBuf> {
    urls.iter()
        .filter_map(|url| url.to_file_path().ok())
        .filter(|path| path.is_file())
        .collect()
}

/// Keeps the paths for the frontend, tells it, and brings the window to the front.
pub fn receive(app: &tauri::AppHandle, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    app.state::<OpenedFiles>().0.lock().extend(paths);
    // A page that has not loaded yet misses this. It takes the paths when it starts.
    let _ = app.emit(OPEN_FILES_EVENT, ());
    crate::show_main_window(app);
}

#[cfg(test)]
mod tests {
    use super::{OpenedFiles, file_args};

    #[test]
    fn only_existing_files_are_kept() {
        let dir = std::env::temp_dir();
        let file = dir.join("sql-pro-open-file-test.db");
        std::fs::write(&file, b"").unwrap();
        let found = file_args(
            ["-flag", "sql-pro-open-file-test.db", "missing.db", "."],
            &dir,
        );
        assert_eq!(found, vec![file.clone()]);
        // An absolute path does not change.
        assert_eq!(file_args([file.as_os_str()], &dir), vec![file.clone()]);
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn take_empties_the_queue() {
        let files = OpenedFiles::default();
        files.0.lock().push("a.db".into());
        assert_eq!(files.take().len(), 1);
        assert_eq!(files.take(), Vec::<std::path::PathBuf>::new());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn urls_become_paths() {
        let file = std::env::temp_dir().join("sql-pro-open-url-test.db");
        std::fs::write(&file, b"").unwrap();
        let urls = [
            tauri::Url::from_file_path(&file).unwrap(),
            tauri::Url::parse("https://example.com/a.db").unwrap(),
        ];
        assert_eq!(super::file_urls(&urls), vec![file.clone()]);
        std::fs::remove_file(file).unwrap();
    }
}
