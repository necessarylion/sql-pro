//! The Settings window: a real window of its own, like the Preferences of a macOS app.

use tauri::{
    AppHandle, LogicalSize, Manager as _, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::constants::{
    MAIN_WINDOW, SETTINGS_WINDOW, SETTINGS_WINDOW_HEIGHT, SETTINGS_WINDOW_MAX_SCREEN_SHARE,
    SETTINGS_WINDOW_MIN_HEIGHT, SETTINGS_WINDOW_TITLE, SETTINGS_WINDOW_URL, SETTINGS_WINDOW_WIDTH,
};

/// Opens the Settings window. An open one only comes to the front.
///
/// The new window starts hidden. Its page shows it after the first paint,
/// so it never flashes in the wrong theme. Closing it really closes it.
pub fn open(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        // These only fail when the window is gone. Then there is nothing to show.
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    if let Err(error) = build(app) {
        eprintln!("SQL Pro: no Settings window: {error}");
    }
}

/// Gives the Settings window the height of its content, then shows it.
///
/// The page measures its content and its own viewport. Rust clamps the height,
/// because only Rust knows the real screen: a hidden webview can report a wrong one.
///
/// On macOS the size that `set_size` takes also holds the title bar, so the page
/// would be cut by its height. The gap between that size and the viewport of the
/// page is the title bar, so it is added. Where there is no gap, nothing is added.
/// Another window is left alone.
pub fn fit(window: &WebviewWindow, content_height: f64, viewport_height: f64) -> tauri::Result<()> {
    if window.label() != SETTINGS_WINDOW {
        return Ok(());
    }
    let monitor = match window.current_monitor()? {
        Some(monitor) => Some(monitor),
        None => window.primary_monitor()?,
    };
    let screen_height = monitor.map(|monitor| {
        monitor
            .size()
            .to_logical::<f64>(monitor.scale_factor())
            .height
    });
    let scale = window.scale_factor()?;
    let size = window.inner_size()?.to_logical::<f64>(scale).height;
    let title_bar = (size - viewport_height).max(0.0);
    let height = fit_height(content_height, screen_height) + title_bar;
    // The window shows also when the size fails, at its start height. Else nothing would show it.
    let sized = window.set_size(LogicalSize::new(SETTINGS_WINDOW_WIDTH, height));
    window.show()?;
    window.set_focus()?;
    sized
}

/// The window height for content of `content` logical pixels: never too low,
/// never above most of the screen. Without a known screen there is no top limit.
fn fit_height(content: f64, screen_height: Option<f64>) -> f64 {
    let most = screen_height.map_or(f64::MAX, |height| height * SETTINGS_WINDOW_MAX_SCREEN_SHARE);
    content.ceil().min(most).max(SETTINGS_WINDOW_MIN_HEIGHT)
}

fn build(app: &AppHandle) -> tauri::Result<()> {
    let url = WebviewUrl::App(SETTINGS_WINDOW_URL.into());
    let builder = WebviewWindowBuilder::new(app, SETTINGS_WINDOW, url)
        .title(SETTINGS_WINDOW_TITLE)
        .inner_size(SETTINGS_WINDOW_WIDTH, SETTINGS_WINDOW_HEIGHT)
        // The page fits the height to its content. The user does not resize it.
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .skip_taskbar(true)
        .visible(false);
    let builder = place(app, builder)?;
    builder.build()?;
    Ok(())
}

/// Puts the window in the middle of the main window, or of the screen without one.
///
/// On Windows and Linux the main window also becomes its owner. Then it stays
/// in front of the main window and has no taskbar button. macOS leaves this out:
/// there a child window would move and hide with the main window.
// macOS never fails here. Windows and Linux can fail to set the owner.
#[cfg_attr(target_os = "macos", allow(clippy::unnecessary_wraps))]
fn place<'a>(
    app: &'a AppHandle,
    builder: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
) -> tauri::Result<WebviewWindowBuilder<'a, tauri::Wry, AppHandle>> {
    let Some(main) = app.get_webview_window(MAIN_WINDOW) else {
        return Ok(builder.center());
    };
    let (Ok(position), Ok(size), Ok(scale)) = (
        main.outer_position(),
        main.outer_size(),
        main.scale_factor(),
    ) else {
        return Ok(builder.center());
    };
    let position = position.to_logical::<f64>(scale);
    let size = size.to_logical::<f64>(scale);
    let x = position.x + (size.width - SETTINGS_WINDOW_WIDTH) / 2.0;
    let y = position.y + (size.height - SETTINGS_WINDOW_HEIGHT) / 2.0;
    let builder = builder.position(x, y);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.parent(&main)?;
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_fits_the_content_between_the_limits() {
        assert!((fit_height(250.4, Some(1000.0)) - 251.0).abs() < f64::EPSILON);
        assert!((fit_height(10.0, Some(1000.0)) - SETTINGS_WINDOW_MIN_HEIGHT).abs() < f64::EPSILON);
        // Above 85% of the screen the window stops growing, and the page scrolls.
        assert!((fit_height(2000.0, Some(1000.0)) - 850.0).abs() < f64::EPSILON);
        assert!((fit_height(400.0, None) - 400.0).abs() < f64::EPSILON);
    }
}
