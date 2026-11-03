use std::thread;
use std::time::Duration;

use forma_core::settings::ThemePreference;
use tauri::window::Color;
use tauri::{Runtime, Theme, WebviewWindow, Window};

pub const MAIN: &str = "main";

/// How long to wait for the frontend's `app_ready` before showing the window anyway.
const READY_TIMEOUT: Duration = Duration::from_secs(3);

/// Window background colors matching `--background` in src/styles/tokens.css.
/// They show while the webview resizes or loads, which avoids white flashes in dark mode.
const LIGHT_BACKGROUND: Color = Color(255, 255, 255, 255);
const DARK_BACKGROUND: Color = Color(28, 28, 27, 255);

/// Sets the native window appearance (title bar, traffic lights, and the
/// webview's `prefers-color-scheme`) to match the user's preference.
pub fn apply_theme<R: Runtime>(window: &WebviewWindow<R>, preference: ThemePreference) {
    let theme = match preference {
        ThemePreference::System => None,
        ThemePreference::Light => Some(Theme::Light),
        ThemePreference::Dark => Some(Theme::Dark),
    };
    warn_on_err(window.set_theme(theme), "set window theme");
    match window.theme() {
        Ok(effective) => warn_on_err(
            window.set_background_color(Some(background_for(effective))),
            "set window background",
        ),
        Err(err) => log::warn!("couldn't read the window theme: {err}"),
    }
}

pub fn sync_background<R: Runtime>(window: &Window<R>, theme: Theme) {
    warn_on_err(
        window.set_background_color(Some(background_for(theme))),
        "set window background",
    );
}

fn background_for(theme: Theme) -> Color {
    match theme {
        Theme::Dark => DARK_BACKGROUND,
        _ => LIGHT_BACKGROUND,
    }
}

pub fn focus<R: Runtime>(window: &WebviewWindow<R>) {
    warn_on_err(window.unminimize(), "unminimize window");
    warn_on_err(window.show(), "show window");
    warn_on_err(window.set_focus(), "focus window");
}

/// Window-API failures are cosmetic for the user but must be visible in the log.
pub fn warn_on_err<T>(result: tauri::Result<T>, action: &str) {
    if let Err(err) = result {
        log::warn!("couldn't {action}: {err}");
    }
}

/// The window starts hidden so the first frame already has the right theme.
/// If the frontend never reports ready (for example, a script error), show it
/// anyway so the app is never invisible.
pub fn show_if_frontend_stalls<R: Runtime>(window: WebviewWindow<R>) {
    thread::spawn(move || {
        thread::sleep(READY_TIMEOUT);
        if !window.is_visible().unwrap_or(true) {
            log::warn!("frontend didn't report ready within {READY_TIMEOUT:?}; showing the window");
            warn_on_err(window.show(), "show window");
        }
    });
}

#[cfg(test)]
mod tests {
    /// `data-tauri-drag-region` invokes these window commands: `start_dragging` on
    /// mousedown and, on macOS, `internal_toggle_maximize` on double-click
    /// (tauri/src/window/scripts/drag.js). Without them the title bar can't
    /// drag or zoom the window.
    #[test]
    fn capabilities_allow_title_bar_drag_and_zoom() {
        let capabilities = include_str!("../capabilities/default.json");
        for permission in [
            "\"core:window:allow-start-dragging\"",
            "\"core:window:allow-internal-toggle-maximize\"",
        ] {
            assert!(
                capabilities.contains(permission),
                "{permission} missing from capabilities/default.json"
            );
        }
    }
}
