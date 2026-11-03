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
    let _ = window.set_theme(theme);
    if let Ok(effective) = window.theme() {
        let _ = window.set_background_color(Some(background_for(effective)));
    }
}

pub fn sync_background<R: Runtime>(window: &Window<R>, theme: Theme) {
    let _ = window.set_background_color(Some(background_for(theme)));
}

fn background_for(theme: Theme) -> Color {
    match theme {
        Theme::Dark => DARK_BACKGROUND,
        _ => LIGHT_BACKGROUND,
    }
}

pub fn focus<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// The window starts hidden so the first frame already has the right theme.
/// If the frontend never reports ready (for example, a script error), show it
/// anyway so the app is never invisible.
pub fn show_if_frontend_stalls<R: Runtime>(window: WebviewWindow<R>) {
    thread::spawn(move || {
        thread::sleep(READY_TIMEOUT);
        if !window.is_visible().unwrap_or(true) {
            let _ = window.show();
        }
    });
}
