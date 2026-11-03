//! App-level settings stored at `<app config dir>/settings.json` (DATA_MODEL §11.1).
//!
//! Settings are preferences, not user content. A corrupt file is moved aside
//! and replaced by defaults, and keys this build doesn't know are kept so a
//! newer build's settings survive a round trip through this one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::atomic::{remove_stale_temps, write_atomic};
use crate::error::{ErrorCode, ErrorPayload};

pub const SETTINGS_FORMAT_VERSION: u32 = 1;
pub const SETTINGS_FILE_NAME: &str = "settings.json";

pub const SIDEBAR_WIDTH: WidthRange = WidthRange {
    min: 200,
    max: 480,
    default: 260,
};
pub const AI_PANEL_WIDTH: WidthRange = WidthRange {
    min: 300,
    max: 640,
    default: 360,
};

#[derive(Debug, Clone, Copy)]
pub struct WidthRange {
    pub min: u32,
    pub max: u32,
    pub default: u32,
}

impl WidthRange {
    fn clamp(self, width: u32) -> u32 {
        width.clamp(self.min, self.max)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AppearanceSettings {
    pub theme: ThemePreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PanelSettings {
    pub visible: bool,
    pub width: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct LayoutSettings {
    pub sidebar: PanelSettings,
    pub ai_panel: PanelSettings,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            sidebar: PanelSettings {
                visible: true,
                width: SIDEBAR_WIDTH.default,
            },
            ai_panel: PanelSettings {
                visible: false,
                width: AI_PANEL_WIDTH.default,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AppSettings {
    pub format_version: u32,
    pub appearance: AppearanceSettings,
    pub layout: LayoutSettings,
    /// Keys this build doesn't know, preserved verbatim. Never sent to the webview.
    #[serde(flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    #[ts(skip)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            format_version: SETTINGS_FORMAT_VERSION,
            appearance: AppearanceSettings::default(),
            layout: LayoutSettings::default(),
            unknown: serde_json::Map::new(),
        }
    }
}

impl AppSettings {
    /// Clamps values into their valid ranges.
    fn normalized(mut self) -> Self {
        self.layout.sidebar.width = SIDEBAR_WIDTH.clamp(self.layout.sidebar.width);
        self.layout.ai_panel.width = AI_PANEL_WIDTH.clamp(self.layout.ai_panel.width);
        self
    }

    /// A copy without the preserved unknown keys, which stay in Rust.
    pub fn for_ui(&self) -> AppSettings {
        AppSettings {
            unknown: serde_json::Map::new(),
            ..self.clone()
        }
    }

    /// Replaces the fields this build manages, keeping `format_version` and unknown keys.
    fn apply_update(&mut self, update: SettingsUpdate) {
        self.appearance = update.appearance;
        self.layout = update.layout;
        *self = std::mem::take(self).normalized();
    }
}

/// The fields the UI may change. `format_version` and unknown keys stay with the store.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct SettingsUpdate {
    pub appearance: AppearanceSettings,
    pub layout: LayoutSettings,
}

/// What the UI receives: the settings plus an optional one-time notice.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SettingsSnapshot {
    pub settings: AppSettings,
    /// Set when the stored file had to be reset or can't be written by this build.
    pub notice: Option<String>,
    /// True when the stored file comes from a newer Forma and won't be overwritten.
    pub read_only: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not access settings at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("settings were written by a newer version of Forma and are read-only")]
    ReadOnly,
}

impl From<&SettingsError> for ErrorPayload {
    fn from(err: &SettingsError) -> Self {
        let code = match err {
            SettingsError::Io { .. } => ErrorCode::Io,
            SettingsError::ReadOnly => ErrorCode::ReadOnly,
        };
        ErrorPayload::new(code, err.to_string())
    }
}

/// Owns the settings file. One instance per app process.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    settings: AppSettings,
    read_only: bool,
}

impl SettingsStore {
    /// Loads settings from `dir/settings.json`, creating `dir` if needed.
    ///
    /// Never fails because of bad file contents: a corrupt file is renamed to
    /// `settings.json.corrupt-<unix-seconds>` and defaults are used.
    pub fn load(dir: &Path) -> Result<(Self, Option<String>), SettingsError> {
        let path = dir.join(SETTINGS_FILE_NAME);
        let io_err = |source| SettingsError::Io {
            path: path.clone(),
            source,
        };

        fs::create_dir_all(dir).map_err(io_err)?;
        remove_stale_temps(&path).map_err(io_err)?;

        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok((Self::with(path, AppSettings::default(), false), None));
            }
            Err(e) => return Err(io_err(e)),
        };

        match serde_json::from_slice::<AppSettings>(&bytes) {
            Ok(settings) => {
                let read_only = settings.format_version > SETTINGS_FORMAT_VERSION;
                let notice = read_only.then(|| {
                    "Settings were saved by a newer version of Forma. Changes made here won't be saved."
                        .to_string()
                });
                Ok((Self::with(path, settings.normalized(), read_only), notice))
            }
            Err(_) => {
                let backup = path.with_file_name(format!(
                    "{SETTINGS_FILE_NAME}.corrupt-{}",
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or_default()
                ));
                fs::rename(&path, &backup).map_err(io_err)?;
                let notice = format!(
                    "Your settings file couldn't be read, so Forma reset it to defaults. The old file was kept as {}.",
                    backup.file_name().unwrap_or_default().to_string_lossy()
                );
                Ok((
                    Self::with(path, AppSettings::default(), false),
                    Some(notice),
                ))
            }
        }
    }

    fn with(path: PathBuf, settings: AppSettings, read_only: bool) -> Self {
        Self {
            path,
            settings,
            read_only,
        }
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Applies `update`, persists atomically, and returns the stored result.
    /// On a write failure the in-memory settings are left unchanged.
    pub fn update(&mut self, update: SettingsUpdate) -> Result<&AppSettings, SettingsError> {
        if self.read_only {
            return Err(SettingsError::ReadOnly);
        }
        let mut next = self.settings.clone();
        next.apply_update(update);
        let mut bytes = serde_json::to_vec_pretty(&next).expect("settings always serialize");
        bytes.push(b'\n');
        write_atomic(&self.path, &bytes).map_err(|source| SettingsError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.settings = next;
        Ok(&self.settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_with(theme: ThemePreference, sidebar_width: u32) -> SettingsUpdate {
        let mut layout = LayoutSettings::default();
        layout.sidebar.width = sidebar_width;
        SettingsUpdate {
            appearance: AppearanceSettings { theme },
            layout,
        }
    }

    #[test]
    fn missing_file_loads_defaults_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let (store, notice) = SettingsStore::load(dir.path()).unwrap();

        assert_eq!(store.settings(), &AppSettings::default());
        assert!(notice.is_none());
        assert!(!store.path().exists(), "loading must not create the file");
    }

    #[test]
    fn defaults_match_spec() {
        let s = AppSettings::default();
        assert_eq!(s.appearance.theme, ThemePreference::System);
        assert!(s.layout.sidebar.visible);
        assert!(!s.layout.ai_panel.visible, "AI panel is hidden by default");
    }

    #[test]
    fn update_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        store
            .update(update_with(ThemePreference::Dark, 300))
            .unwrap();

        let (reloaded, notice) = SettingsStore::load(dir.path()).unwrap();
        assert!(notice.is_none());
        assert_eq!(reloaded.settings().appearance.theme, ThemePreference::Dark);
        assert_eq!(reloaded.settings().layout.sidebar.width, 300);
    }

    #[test]
    fn widths_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        let stored = store
            .update(update_with(ThemePreference::Light, 5_000))
            .unwrap();
        assert_eq!(stored.layout.sidebar.width, SIDEBAR_WIDTH.max);

        fs::write(
            store.path(),
            r#"{"format_version":1,"layout":{"sidebar":{"visible":true,"width":1},"ai_panel":{"visible":false,"width":99999}}}"#,
        )
        .unwrap();
        let (reloaded, _) = SettingsStore::load(dir.path()).unwrap();
        assert_eq!(reloaded.settings().layout.sidebar.width, SIDEBAR_WIDTH.min);
        assert_eq!(
            reloaded.settings().layout.ai_panel.width,
            AI_PANEL_WIDTH.max
        );
    }

    #[test]
    fn corrupt_file_is_kept_aside_and_defaults_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE_NAME);
        fs::write(&path, b"{ not json").unwrap();

        let (store, notice) = SettingsStore::load(dir.path()).unwrap();
        assert_eq!(store.settings(), &AppSettings::default());
        assert!(notice.unwrap().contains("reset it to defaults"));
        assert!(!path.exists());

        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("settings.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read(dir.path().join(&backups[0])).unwrap(),
            b"{ not json"
        );
    }

    #[test]
    fn wrong_value_types_count_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            br#"{"appearance":{"theme":"purple"}}"#,
        )
        .unwrap();
        let (store, notice) = SettingsStore::load(dir.path()).unwrap();
        assert_eq!(store.settings().appearance.theme, ThemePreference::System);
        assert!(notice.is_some());
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            br#"{"appearance":{"theme":"dark"}}"#,
        )
        .unwrap();
        let (store, notice) = SettingsStore::load(dir.path()).unwrap();
        assert!(notice.is_none());
        assert_eq!(store.settings().appearance.theme, ThemePreference::Dark);
        assert_eq!(store.settings().layout, LayoutSettings::default());
    }

    #[test]
    fn unknown_keys_survive_an_update() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            br#"{"format_version":1,"ai":{"enabled":true},"workspace":{"last_path":"/x"}}"#,
        )
        .unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        store
            .update(update_with(ThemePreference::Light, 260))
            .unwrap();

        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(raw["ai"]["enabled"], true);
        assert_eq!(raw["workspace"]["last_path"], "/x");
        assert_eq!(raw["appearance"]["theme"], "light");

        let for_ui = serde_json::to_value(store.settings().for_ui()).unwrap();
        assert!(
            for_ui.get("ai").is_none(),
            "unknown keys must not reach the webview"
        );
        assert!(for_ui.get("workspace").is_none());
    }

    #[test]
    fn newer_format_is_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE_NAME);
        let original = br#"{"format_version":2,"appearance":{"theme":"dark"},"future":true}"#;
        fs::write(&path, original).unwrap();

        let (mut store, notice) = SettingsStore::load(dir.path()).unwrap();
        assert!(store.is_read_only());
        assert!(notice.is_some());
        assert_eq!(store.settings().appearance.theme, ThemePreference::Dark);
        assert!(matches!(
            store.update(update_with(ThemePreference::Light, 260)),
            Err(SettingsError::ReadOnly)
        ));
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn failed_write_keeps_previous_settings() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        store
            .update(update_with(ThemePreference::Dark, 260))
            .unwrap();

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = store.update(update_with(ThemePreference::Light, 260));
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

        assert!(matches!(result, Err(SettingsError::Io { .. })));
        assert_eq!(store.settings().appearance.theme, ThemePreference::Dark);
    }

    #[test]
    fn stale_temp_files_are_cleaned_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join(".settings.json.forma-tmp-123-456-0");
        fs::write(&stale, b"partial").unwrap();
        SettingsStore::load(dir.path()).unwrap();
        assert!(!stale.exists());
    }
}
