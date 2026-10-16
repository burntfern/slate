//! Persisted settings.
//!
//! Everything lives under `%USERPROFILE%\.burntfern\`:
//!
//! ```text
//! .burntfern\
//!   config.json          # shared across every burntfern app
//!   slate\
//!     settings.json      # Slate-specific
//!     groups.json        # saved groups (later)
//!     logs\  backups\
//! ```
//!
//! Values cascade **baked defaults <- shared `config.json` <- `slate\settings.json`**.
//! Writes are atomic (temp + rename); a corrupt file is moved to `backups\` and
//! the app falls back to defaults instead of crashing.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// On-disk settings version. Bump when the shape changes and add a migration.
pub const SCHEMA_VERSION: u32 = 1;

/// Resolved settings handed to the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)] // accent/schema_version land in later phases
pub struct Settings {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default = "theme_default")]
    pub theme: String,
    #[serde(default = "accent_default")]
    pub accent: String,
    #[serde(default)]
    pub tabs: Tabs,
}

/// Tab-strip preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tabs {
    #[serde(default = "yes")]
    pub icons: bool,
    #[serde(default = "yes")]
    pub count_badge: bool,
    #[serde(default = "yes")]
    pub hover_preview: bool,
}

impl Default for Tabs {
    fn default() -> Self {
        Self {
            icons: true,
            count_badge: true,
            hover_preview: true,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            theme: theme_default(),
            accent: accent_default(),
            tabs: Tabs::default(),
        }
    }
}

/// A file whose keys are all optional, so a partial file only overrides what it
/// actually sets. Used for both the shared and the app file.
#[derive(Debug, Default, Deserialize)]
struct PartialSettings {
    theme: Option<String>,
    accent: Option<String>,
    tabs: Option<PartialTabs>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialTabs {
    icons: Option<bool>,
    count_badge: Option<bool>,
    hover_preview: Option<bool>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}
fn theme_default() -> String {
    "system".to_string()
}
fn accent_default() -> String {
    "blue".to_string()
}
fn yes() -> bool {
    true
}

// --- paths -------------------------------------------------------------------

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(std::env::temp_dir)
}

/// `%USERPROFILE%\.burntfern` — shared root for every burntfern app.
pub fn burntfern_root() -> PathBuf {
    home_dir().join(".burntfern")
}

/// Slate's own directory: `.burntfern\slate`.
pub fn app_dir() -> PathBuf {
    burntfern_root().join("slate")
}

pub fn shared_config_path() -> PathBuf {
    burntfern_root().join("config.json")
}

pub fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

#[allow(dead_code)] // used by saved groups (later phase)
pub fn groups_path() -> PathBuf {
    app_dir().join("groups.json")
}

pub fn logs_dir() -> PathBuf {
    app_dir().join("logs")
}

pub fn backups_dir() -> PathBuf {
    app_dir().join("backups")
}

/// The repo/install `configs\` directory holding the default templates.
fn template_dir() -> Option<PathBuf> {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("configs")));
    let in_cwd = std::env::current_dir().ok().map(|dir| dir.join("configs"));
    [beside_exe, in_cwd]
        .into_iter()
        .flatten()
        .find(|dir| dir.is_dir())
}

/// Create the directory tree and seed missing files from the repo templates
/// (falling back to baked defaults). Safe to call every start.
pub fn ensure() {
    for dir in [burntfern_root(), app_dir(), logs_dir(), backups_dir()] {
        let _ = std::fs::create_dir_all(dir);
    }
    if !shared_config_path().exists() {
        seed("config.json", &shared_config_path(), &SharedSeed);
    }
    if !settings_path().exists() {
        seed("slate\\settings.json", &settings_path(), &AppSeed);
    }
}

/// Writes `path` from `configs\<template>` if present, else from `seed_content`.
trait Seed {
    fn content(&self) -> String;
}

struct SharedSeed;
struct AppSeed;

impl Seed for SharedSeed {
    fn content(&self) -> String {
        "{ \"schema_version\": 1, \"theme\": \"system\", \"accent\": \"blue\" }\n".to_string()
    }
}

impl Seed for AppSeed {
    fn content(&self) -> String {
        let settings = Settings::default();
        serde_json::to_string_pretty(&settings)
            .map(|mut s| {
                s.push('\n');
                s
            })
            .unwrap_or_else(|_| "{}\n".to_string())
    }
}

fn seed(template_rel: &str, dest: &Path, fallback: &dyn Seed) {
    if let Some(dir) = template_dir() {
        if let Ok(bytes) = std::fs::read(dir.join(template_rel)) {
            let _ = std::fs::write(dest, bytes);
            return;
        }
    }
    let _ = std::fs::write(dest, fallback.content());
}

/// Load settings, merging defaults <- shared `config.json` <- `slate\settings.json`.
pub fn load() -> Settings {
    let mut merged = Settings::default();
    if let Some(partial) = read_partial(&shared_config_path()) {
        apply(&mut merged, partial);
    }
    if let Some(partial) = read_partial(&settings_path()) {
        apply(&mut merged, partial);
    }
    merged
}

/// Persist settings atomically, creating the directory if needed.
pub fn save(settings: &Settings) {
    let _ = std::fs::create_dir_all(app_dir());
    let Ok(mut json) = serde_json::to_string_pretty(settings) else {
        return;
    };
    json.push('\n');
    atomic_write(&settings_path(), json.as_bytes());
}

/// Read one file into its partial form; a corrupt file is backed up and ignored.
fn read_partial(path: &Path) -> Option<PartialSettings> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            backup(path);
            return None;
        }
    };
    match serde_json::from_value(migrate(value)) {
        Ok(partial) => Some(partial),
        Err(_) => {
            backup(path);
            None
        }
    }
}

/// Overlay a partial file onto resolved settings (only set keys win).
fn apply(settings: &mut Settings, partial: PartialSettings) {
    if let Some(theme) = partial.theme {
        settings.theme = theme;
    }
    if let Some(accent) = partial.accent {
        settings.accent = accent;
    }
    if let Some(tabs) = partial.tabs {
        if let Some(icons) = tabs.icons {
            settings.tabs.icons = icons;
        }
        if let Some(count_badge) = tabs.count_badge {
            settings.tabs.count_badge = count_badge;
        }
        if let Some(hover_preview) = tabs.hover_preview {
            settings.tabs.hover_preview = hover_preview;
        }
    }
}

/// Bring an older on-disk shape forward. Pure: no IO, unit-tested.
pub fn migrate(mut value: serde_json::Value) -> serde_json::Value {
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if version < 1 {
        value["schema_version"] = SCHEMA_VERSION.into();
    }
    // Future: `if version < 2 { ... }`.
    value
}

/// Write `bytes` to `path` via a temp file + rename so a failure never clobbers
/// the last good file.
fn atomic_write(path: &Path, bytes: &[u8]) -> bool {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_err() {
        return false;
    }
    std::fs::rename(&tmp, path).is_ok()
}

/// Move a corrupt file into `backups\` with a timestamp, so it is preserved but
/// no longer loaded. Best-effort.
fn backup(path: &Path) {
    let Some(name) = path.file_name() else {
        return;
    };
    let _ = std::fs::create_dir_all(backups_dir());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = backups_dir().join(format!("{}.{stamp}", name.to_string_lossy()));
    let _ = std::fs::rename(path, dest);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_all_on() {
        let s = Settings::default();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert!(s.tabs.icons && s.tabs.count_badge && s.tabs.hover_preview);
    }

    #[test]
    fn partial_overrides_only_what_it_sets() {
        let mut s = Settings::default();
        apply(
            &mut s,
            PartialSettings {
                theme: Some("dark".into()),
                accent: None,
                tabs: Some(PartialTabs {
                    icons: Some(false),
                    count_badge: None,
                    hover_preview: None,
                }),
            },
        );
        assert_eq!(s.theme, "dark");
        assert_eq!(s.accent, "blue"); // untouched
        assert!(!s.tabs.icons); // overridden
        assert!(s.tabs.count_badge); // untouched
    }

    #[test]
    fn shared_then_app_precedence() {
        let mut s = Settings::default();
        apply(
            &mut s,
            PartialSettings {
                theme: Some("light".into()),
                ..Default::default()
            },
        );
        apply(
            &mut s,
            PartialSettings {
                theme: Some("dark".into()),
                ..Default::default()
            },
        );
        assert_eq!(s.theme, "dark");
    }

    #[test]
    fn migrate_stamps_missing_version() {
        let out = migrate(serde_json::json!({ "theme": "dark" }));
        assert_eq!(out["schema_version"], SCHEMA_VERSION);
    }

    #[test]
    fn settings_round_trip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, s.theme);
        assert_eq!(back.tabs.icons, s.tabs.icons);
    }
}
