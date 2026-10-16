use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::schema::{SavedSet, SavedSets, Settings, SCHEMA_VERSION};

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
    show_bar: Option<bool>,
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

/// `slate\sets.json` — the saved tab sets.
pub fn sets_path() -> PathBuf {
    app_dir().join("sets.json")
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
    if !sets_path().exists() {
        seed("slate\\sets.json", &sets_path(), &SetsSeed);
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

struct SetsSeed;

impl Seed for SetsSeed {
    fn content(&self) -> String {
        "{ \"schema_version\": 1, \"sets\": [] }\n".to_string()
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

/// Load the saved tab sets. A missing file yields an empty set; a corrupt one is
/// backed up and treated as empty rather than crashing.
pub fn load_sets() -> SavedSets {
    let path = sets_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return SavedSets::default();
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) => parse_sets(value),
        Err(_) => {
            backup(&path);
            SavedSets::default()
        }
    }
}

/// Parse `sets.json`, keeping every well-formed set and skipping bad entries so a
/// single malformed set never drops the others.
fn parse_sets(value: serde_json::Value) -> SavedSets {
    let value = migrate(value);
    let mut out = SavedSets::default();
    if let Some(version) = value.get("schema_version").and_then(|v| v.as_u64()) {
        out.schema_version = version as u32;
    }
    if let Some(items) = value.get("sets").and_then(|v| v.as_array()) {
        out.sets = items
            .iter()
            .filter_map(|item| serde_json::from_value::<SavedSet>(item.clone()).ok())
            .collect();
    }
    out
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
        if let Some(show_bar) = tabs.show_bar {
            settings.tabs.show_bar = show_bar;
        }
    }
}

/// Bring an older on-disk shape forward. Pure: no IO, unit-tested.
fn migrate(mut value: serde_json::Value) -> serde_json::Value {
    // Valid JSON that is not an object (a bare number/string/array/null) has no
    // keys to migrate; the caller's deserialize step rejects it and falls back.
    if !value.is_object() {
        return value;
    }
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
                    show_bar: None,
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
    fn migrate_leaves_non_object_untouched() {
        // Valid JSON of the wrong shape must not panic during migration.
        assert_eq!(migrate(serde_json::json!(123)).as_u64(), Some(123));
        assert!(migrate(serde_json::json!(null)).is_null());
    }

    #[test]
    fn parse_sets_skips_bad_entries() {
        let value = serde_json::json!({
            "schema_version": 1,
            "sets": [
                { "name": "Good", "tabs": [] },
                { "tabs": "not-an-array" },
                42,
                { "name": "AlsoGood", "tabs": [] }
            ]
        });
        let sets = parse_sets(value);
        let names: Vec<_> = sets.sets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Good", "AlsoGood"]);
    }
}
