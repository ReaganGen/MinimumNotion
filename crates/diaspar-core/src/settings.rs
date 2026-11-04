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
    /// Managed by Rust only; the UI can't change it through `SettingsUpdate`.
    pub workspace: WorkspaceSettings,
    /// Changed only through the AI commands, which validate it and keep the
    /// Keychain in step. Never holds a credential (DATA_MODEL §11.1).
    pub ai: AiSettings,
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
            workspace: WorkspaceSettings::default(),
            ai: AiSettings::default(),
            unknown: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct WorkspaceSettings {
    /// The workspace folder opened last, reopened on launch.
    pub last_path: Option<String>,
}

/// Which wire protocol and defaults a provider uses (ARCHITECTURE §9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ProviderKind {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "lmstudio")]
    LmStudio,
}

/// Per-provider generation settings. `None` means "the provider's default".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct GenerationOptions {
    pub temperature: Option<f64>,
    pub max_output_tokens: Option<u32>,
    pub context_window_override: Option<u32>,
    /// Whether the model can call tools, when the user knows better than the
    /// provider reports (OpenAI reports nothing). `None`: what the provider says.
    pub tool_calling_override: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    /// Empty until the user picks a model.
    #[serde(default)]
    pub model: String,
    /// Whether a key was stored in the Keychain. The key itself never is here.
    #[serde(default)]
    pub has_credential: bool,
    #[serde(default)]
    pub options: GenerationOptions,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AiSettings {
    /// AI is off until the user turns it on.
    pub enabled: bool,
    pub active_provider: Option<String>,
    pub providers: Vec<ProviderConfig>,
    /// Provider ID → when the user agreed that messages, and the note excerpts
    /// they need, may leave this Mac (RFC 3339). Agreements given before notes
    /// could be sent were under another key and no longer count (ADR-042).
    pub cloud_consent: std::collections::BTreeMap<String, String>,
}

impl AiSettings {
    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn active(&self) -> Option<&ProviderConfig> {
        self.active_provider
            .as_deref()
            .and_then(|id| self.provider(id))
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
    /// Set when changes can't be saved, explaining why. `None` means writable.
    pub read_only_reason: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not access settings at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("settings are read-only")]
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

impl From<SettingsError> for ErrorPayload {
    fn from(err: SettingsError) -> Self {
        Self::from(&err)
    }
}

/// Owns the settings file. One instance per app process.
#[derive(Debug)]
pub struct SettingsStore {
    path: PathBuf,
    settings: AppSettings,
    /// Why changes can't be saved, or `None` when the file is writable.
    read_only_reason: Option<String>,
}

impl SettingsStore {
    /// Like [`SettingsStore::load`], but never fails. If the settings folder
    /// can't be used, Diaspar still opens: it runs on defaults, read-only, and
    /// the returned notice says why.
    pub fn load_or_fallback(dir: &Path) -> (Self, Option<String>) {
        Self::load(dir).unwrap_or_else(|err| Self::unavailable(&err.to_string()))
    }

    /// Read-only defaults for when the settings folder is unusable. `cause`
    /// is shown to the user. Nothing is ever written.
    pub fn unavailable(cause: &str) -> (Self, Option<String>) {
        let reason = format!(
            "Diaspar couldn't open its settings ({cause}). Defaults are in use, and changes won't be saved."
        );
        let store = Self {
            path: PathBuf::new(),
            settings: AppSettings::default(),
            read_only_reason: Some(reason.clone()),
        };
        (store, Some(reason))
    }

    /// Loads settings from `dir/settings.json`, creating `dir` if needed.
    ///
    /// Never fails because of bad file contents: a corrupt file is renamed to
    /// `settings.json.corrupt-<unix-seconds>` (`-2`, `-3`… when that name is
    /// taken) and defaults are used.
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
                return Ok((Self::with(path, AppSettings::default(), None), None));
            }
            Err(e) => return Err(io_err(e)),
        };

        match serde_json::from_slice::<AppSettings>(&bytes) {
            Ok(settings) => {
                let reason = (settings.format_version > SETTINGS_FORMAT_VERSION).then(|| {
                    "These settings were saved by a newer version of Diaspar, so changes made here won't be saved."
                        .to_string()
                });
                let notice = reason.clone();
                Ok((Self::with(path, settings.normalized(), reason), notice))
            }
            Err(_) => {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or_default();
                // rename(2) replaces, so a second corrupt file in the same
                // second gets its own name instead of replacing the first.
                let backup = (1..)
                    .map(|n| {
                        path.with_file_name(match n {
                            1 => format!("{SETTINGS_FILE_NAME}.corrupt-{stamp}"),
                            n => format!("{SETTINGS_FILE_NAME}.corrupt-{stamp}-{n}"),
                        })
                    })
                    .find(|p| !p.exists())
                    .expect("an unused name");
                fs::rename(&path, &backup).map_err(io_err)?;
                let notice = format!(
                    "Your settings file couldn't be read, so Diaspar reset it to defaults. The old file was kept as {}.",
                    backup.file_name().unwrap_or_default().to_string_lossy()
                );
                Ok((Self::with(path, AppSettings::default(), None), Some(notice)))
            }
        }
    }

    fn with(path: PathBuf, settings: AppSettings, read_only_reason: Option<String>) -> Self {
        Self {
            path,
            settings,
            read_only_reason,
        }
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn read_only_reason(&self) -> Option<&str> {
        self.read_only_reason.as_deref()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Remembers the workspace to reopen on the next launch. Skipped while read-only.
    pub fn set_last_workspace(&mut self, path: Option<String>) -> Result<(), SettingsError> {
        if self.read_only_reason.is_some() {
            return Err(SettingsError::ReadOnly);
        }
        let mut next = self.settings.clone();
        next.workspace.last_path = path;
        self.persist(next)
    }

    fn persist(&mut self, next: AppSettings) -> Result<(), SettingsError> {
        let mut bytes = serde_json::to_vec_pretty(&next).expect("settings always serialize");
        bytes.push(b'\n');
        write_atomic(&self.path, &bytes).map_err(|source| SettingsError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.settings = next;
        Ok(())
    }

    /// Changes the AI settings through `change`, then persists. `change` may
    /// refuse, and nothing is saved when it does or when the write fails.
    pub fn update_ai<E: From<SettingsError>>(
        &mut self,
        change: impl FnOnce(&mut AiSettings) -> Result<(), E>,
    ) -> Result<&AppSettings, E> {
        if self.read_only_reason.is_some() {
            return Err(SettingsError::ReadOnly.into());
        }
        let mut next = self.settings.clone();
        change(&mut next.ai)?;
        self.persist(next)?;
        Ok(&self.settings)
    }

    /// Applies `update`, persists atomically, and returns the stored result.
    /// On a write failure the in-memory settings are left unchanged.
    pub fn update(&mut self, update: SettingsUpdate) -> Result<&AppSettings, SettingsError> {
        if self.read_only_reason.is_some() {
            return Err(SettingsError::ReadOnly);
        }
        let mut next = self.settings.clone();
        next.apply_update(update);
        self.persist(next)?;
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

        // A second corrupt file, almost surely within the same second, keeps
        // the first backup.
        fs::write(&path, b"{ also bad").unwrap();
        SettingsStore::load(dir.path()).unwrap();
        let mut kept: Vec<Vec<u8>> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().contains("settings.json.corrupt-"))
            .map(|p| fs::read(p).unwrap())
            .collect();
        kept.sort();
        assert_eq!(kept, [b"{ also bad".to_vec(), b"{ not json".to_vec()]);
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
            br#"{"format_version":1,"plugins":{"enabled":true},"sync":{"server":"/x"}}"#,
        )
        .unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        store
            .update(update_with(ThemePreference::Light, 260))
            .unwrap();

        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(raw["plugins"]["enabled"], true);
        assert_eq!(raw["sync"]["server"], "/x");
        assert_eq!(raw["appearance"]["theme"], "light");

        let for_ui = serde_json::to_value(store.settings().for_ui()).unwrap();
        assert!(
            for_ui.get("plugins").is_none(),
            "unknown keys must not reach the webview"
        );
        assert!(for_ui.get("sync").is_none());
    }

    #[test]
    fn newer_format_is_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE_NAME);
        let original = br#"{"format_version":2,"appearance":{"theme":"dark"},"future":true}"#;
        fs::write(&path, original).unwrap();

        let (mut store, notice) = SettingsStore::load(dir.path()).unwrap();
        assert!(store.read_only_reason().unwrap().contains("newer version"));
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
        let stale = dir.path().join(".settings.json.diaspar-tmp-123-456-0");
        fs::write(&stale, b"partial").unwrap();
        SettingsStore::load(dir.path()).unwrap();
        assert!(!stale.exists());
    }

    #[test]
    fn last_workspace_persists_and_ui_updates_keep_it() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        store.set_last_workspace(Some("/notes".into())).unwrap();
        store
            .update(update_with(ThemePreference::Dark, 260))
            .unwrap();
        let (reloaded, _) = SettingsStore::load(dir.path()).unwrap();
        assert_eq!(
            reloaded.settings().workspace.last_path.as_deref(),
            Some("/notes")
        );
    }

    #[test]
    fn writable_folder_is_not_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let (store, notice) = SettingsStore::load_or_fallback(dir.path());
        assert!(store.read_only_reason().is_none());
        assert!(notice.is_none());
    }

    #[test]
    fn unreadable_folder_falls_back_to_read_only_defaults() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let settings_dir = dir.path().join("config");
        fs::create_dir(&settings_dir).unwrap();
        let original = br#"{"format_version":1,"appearance":{"theme":"dark"}}"#;
        fs::write(settings_dir.join(SETTINGS_FILE_NAME), original).unwrap();

        fs::set_permissions(&settings_dir, fs::Permissions::from_mode(0o000)).unwrap();
        let (mut store, notice) = SettingsStore::load_or_fallback(&settings_dir);
        let update_refused = matches!(
            store.update(update_with(ThemePreference::Light, 260)),
            Err(SettingsError::ReadOnly)
        );
        fs::set_permissions(&settings_dir, fs::Permissions::from_mode(0o700)).unwrap();

        // The app still opens, on defaults, and says why changes won't stick.
        assert_eq!(store.settings(), &AppSettings::default());
        let reason = store.read_only_reason().unwrap();
        assert!(reason.contains("couldn't open its settings"), "{reason}");
        assert_eq!(notice.as_deref(), Some(reason));
        assert!(update_refused);
        // The existing file is left untouched.
        assert_eq!(
            fs::read(settings_dir.join(SETTINGS_FILE_NAME)).unwrap(),
            original
        );
    }

    #[test]
    fn unavailable_store_is_read_only_defaults() {
        let (mut store, notice) = SettingsStore::unavailable("no home directory");
        assert!(notice.unwrap().contains("no home directory"));
        assert_eq!(store.settings(), &AppSettings::default());
        assert!(matches!(
            store.update(update_with(ThemePreference::Dark, 260)),
            Err(SettingsError::ReadOnly)
        ));
    }

    #[test]
    fn ai_is_off_by_default_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        assert!(!store.settings().ai.enabled);
        store
            .update_ai::<SettingsError>(|ai| {
                ai.enabled = true;
                ai.providers.push(ProviderConfig {
                    id: "p1".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    name: "Local".into(),
                    base_url: "http://localhost:8080/v1".into(),
                    model: "m".into(),
                    has_credential: false,
                    options: GenerationOptions {
                        temperature: Some(0.3),
                        ..GenerationOptions::default()
                    },
                });
                ai.active_provider = Some("p1".into());
                Ok(())
            })
            .unwrap();
        // UI updates don't touch the AI settings.
        store
            .update(update_with(ThemePreference::Dark, 260))
            .unwrap();

        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert_eq!(raw["ai"]["providers"][0]["kind"], "openai_compatible");
        let (reloaded, _) = SettingsStore::load(dir.path()).unwrap();
        let ai = &reloaded.settings().ai;
        assert!(ai.enabled);
        assert_eq!(ai.active().unwrap().options.temperature, Some(0.3));
    }

    #[test]
    fn agreements_from_before_notes_could_be_sent_no_longer_count() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("settings.json"),
            r#"{"format_version": 1, "ai": {"enabled": true, "providers": [],
                "cloud_acknowledged": {"p1": "2026-11-03T10:00:00Z"}}}"#,
        )
        .unwrap();
        let (store, _) = SettingsStore::load(dir.path()).unwrap();
        assert!(store.settings().ai.enabled);
        assert!(store.settings().ai.cloud_consent.is_empty());
    }

    #[test]
    fn a_refused_ai_change_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = SettingsStore::load(dir.path()).unwrap();
        let result = store.update_ai(|ai| {
            ai.enabled = true;
            Err(SettingsError::ReadOnly)
        });
        assert!(result.is_err());
        assert!(!store.settings().ai.enabled);
        assert!(!store.path().exists());
    }
}
