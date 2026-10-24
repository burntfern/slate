use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use super::paths;
use super::schema::{SavedSet, SavedSets, Settings, SCHEMA_VERSION};

#[derive(Debug, Default)]
struct PartialSettings {
    theme: Option<String>,
    accent: Option<String>,
    tabs: Option<PartialTabs>,
    updates: Option<PartialUpdates>,
}

#[derive(Debug, Default)]
struct PartialTabs {
    icons: Option<bool>,
    count_badge: Option<bool>,
    hover_preview: Option<bool>,
    show_bar: Option<bool>,
}

#[derive(Debug, Default)]
struct PartialUpdates {
    latest: Option<String>,
    checked_at: Option<i64>,
}

pub fn burntfern_root() -> PathBuf {
    paths::old_burntfern_root()
}

pub fn app_dir() -> PathBuf {
    paths::data_dir()
}

pub fn shared_config_path() -> PathBuf {
    paths::data_dir().join("config.json")
}

pub fn settings_path() -> PathBuf {
    paths::data_dir().join("settings.json")
}

pub fn sets_path() -> PathBuf {
    paths::data_dir().join("sets.json")
}

pub fn backups_dir() -> PathBuf {
    paths::backups_dir()
}

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

fn migrate_from_burntfern() {
    let new_data = paths::data_dir();
    let old_root = paths::old_burntfern_root();
    let old_slate = paths::old_slate_dir();
    if new_data.exists() {
        return;
    }
    if !old_root.exists() && !old_slate.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&new_data);
    let backup_dir = paths::backups_dir().join(format!(
        "migration-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    let _ = std::fs::create_dir_all(&backup_dir);
    if old_root.exists() {
        let _ = copy_dir(&old_root, &backup_dir.join("burntfern"));
    }
    let files = [
        ("config.json", old_root.join("config.json")),
        ("settings.json", old_slate.join("settings.json")),
        ("sets.json", old_slate.join("sets.json")),
        ("install.json", old_slate.join("install.json")),
    ];
    for (name, old_path) in files {
        if old_path.exists() {
            let _ = std::fs::copy(&old_path, new_data.join(name));
        }
    }
    let old_backups = old_slate.join("backups");
    if old_backups.exists() {
        let _ = copy_dir(&old_backups, &paths::backups_dir());
    }
    let old_updates = old_slate.join("updates");
    if old_updates.exists() {
        let _ = copy_dir(&old_updates, &paths::updates_dir());
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&from, &to)?;
        } else if ty.is_file() {
            let _ = std::fs::copy(&from, &to);
        }
    }
    Ok(())
}

pub fn ensure() {
    let _ = paths::ensure_dirs();
    migrate_from_burntfern();
    let _ = std::fs::create_dir_all(app_dir());
    let _ = std::fs::create_dir_all(paths::backups_dir());
    if !shared_config_path().exists() {
        seed("config.json", &shared_config_path(), &SharedSeed);
    }
    if !settings_path().exists() {
        seed("config.json", &settings_path(), &AppSeed);
    }
    if !sets_path().exists() {
        seed("config.json", &sets_path(), &SetsSeed);
    }
}

trait Seed {
    fn content(&self) -> String;
}

struct SharedSeed;
struct AppSeed;
struct SetsSeed;

impl Seed for SharedSeed {
    fn content(&self) -> String {
        "{ \"schema_version\": 1, \"theme\": \"system\", \"accent\": \"blue\" }\n".to_string()
    }
}

impl Seed for AppSeed {
    fn content(&self) -> String {
        let value = super::save::app_value(&Settings::default());
        serde_json::to_string_pretty(&value)
            .map(|mut s| {
                s.push('\n');
                s
            })
            .unwrap_or_else(|_| "{}\n".to_string())
    }
}

impl Seed for SetsSeed {
    fn content(&self) -> String {
        "{ \"schema_version\": 1, \"sets\": [] }\n".to_string()
    }
}

fn seed(template_rel: &str, dest: &Path, fallback: &dyn Seed) {
    if let Some(dir) = template_dir() {
        if let Ok(bytes) = std::fs::read(dir.join(template_rel)) {
            if write_new(dest, &bytes) {
                return;
            }
        }
    }
    let _ = write_new(dest, fallback.content().as_bytes());
}

fn write_new(dest: &Path, bytes: &[u8]) -> bool {
    if dest.exists() {
        return true;
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(bytes).is_ok()
        }
        Err(_) => false,
    }
}

pub fn load() -> Settings {
    let mut merged = Settings::default();
    if let Some(partial) = read_partial(&shared_config_path(), false) {
        apply(&mut merged, partial);
    }
    if let Some(partial) = read_partial(&settings_path(), true) {
        apply(&mut merged, partial);
    }
    merged
}

pub fn load_sets() -> SavedSets {
    let path = sets_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return SavedSets::default();
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => {
            if schema_version_of(&value) > SCHEMA_VERSION as u64 {
                backup(&path);
                return SavedSets::default();
            }
            parse_sets(value)
        }
        Err(_) => {
            backup(&path);
            SavedSets::default()
        }
    }
}

fn parse_sets(value: Value) -> SavedSets {
    let value = migrate(value);
    let mut out = SavedSets::default();
    if let Some(version) = value.get("schema_version").and_then(|v| v.as_u64()) {
        out.schema_version = u32::try_from(version).unwrap_or(SCHEMA_VERSION);
    }
    if let Some(items) = value.get("sets").and_then(|v| v.as_array()) {
        out.sets = items
            .iter()
            .filter_map(|item| serde_json::from_value::<SavedSet>(item.clone()).ok())
            .collect();
    }
    assign_ids(&mut out.sets);
    out
}

fn assign_ids(sets: &mut [SavedSet]) {
    let mut used: Vec<u64> = Vec::with_capacity(sets.len());
    let mut next = sets
        .iter()
        .map(|s| s.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for set in sets.iter_mut() {
        if set.id == 0 || used.contains(&set.id) {
            set.id = next;
            next = next.saturating_add(1);
        }
        used.push(set.id);
    }
}

fn read_partial(path: &Path, app: bool) -> Option<PartialSettings> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            backup(path);
            return None;
        }
    };
    if schema_version_of(&value) > SCHEMA_VERSION as u64 {
        backup(path);
        return None;
    }
    Some(partial_from_value(migrate(value), app))
}

fn schema_version_of(value: &Value) -> u64 {
    value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

fn partial_from_value(value: Value, app: bool) -> PartialSettings {
    let mut partial = PartialSettings::default();
    if !app {
        partial.theme = str_field(&value, "theme");
        partial.accent = str_field(&value, "accent");
    }
    if let Some(tabs) = value.get("tabs") {
        partial.tabs = Some(PartialTabs {
            icons: bool_field(tabs, "icons"),
            count_badge: bool_field(tabs, "count_badge"),
            hover_preview: bool_field(tabs, "hover_preview"),
            show_bar: bool_field(tabs, "show_bar"),
        });
    }
    if let Some(updates) = value.get("updates") {
        partial.updates = Some(PartialUpdates {
            latest: str_field(updates, "latest"),
            checked_at: i64_field(updates, "checked_at"),
        });
    }
    partial
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(|s| s.to_string())
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key)?.as_bool()
}

fn i64_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key)?.as_i64()
}

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
    if let Some(updates) = partial.updates {
        if updates.latest.is_some() {
            settings.updates.latest = updates.latest;
        }
        if let Some(checked_at) = updates.checked_at {
            settings.updates.checked_at = checked_at;
        }
    }
}

fn migrate(mut value: Value) -> Value {
    if !value.is_object() {
        return value;
    }
    let version = schema_version_of(&value);
    if version < 1 {
        value["schema_version"] = SCHEMA_VERSION.into();
    }
    value
}

fn backup(path: &Path) {
    let Some(name) = path.file_name() else {
        return;
    };
    let _ = std::fs::create_dir_all(backups_dir());
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dest = backups_dir().join(format!("{}.{stamp}.{n}", name.to_string_lossy()));
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
                updates: None,
            },
        );
        assert_eq!(s.theme, "dark");
        assert_eq!(s.accent, "blue");
        assert!(!s.tabs.icons);
        assert!(s.tabs.count_badge);
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
    fn app_partial_ignores_theme_and_accent() {
        let value = serde_json::json!({
            "theme": "dark",
            "accent": "red",
            "tabs": { "icons": false }
        });
        let partial = partial_from_value(value, true);
        assert!(partial.theme.is_none());
        assert!(partial.accent.is_none());
        assert_eq!(partial.tabs.and_then(|t| t.icons), Some(false));
    }

    #[test]
    fn wrong_typed_key_does_not_drop_the_rest() {
        let value = serde_json::json!({
            "theme": 123,
            "accent": "red",
            "tabs": { "icons": "yes", "count_badge": false }
        });
        let partial = partial_from_value(value, false);
        assert!(partial.theme.is_none());
        assert_eq!(partial.accent.as_deref(), Some("red"));
        let tabs = partial.tabs.unwrap();
        assert_eq!(tabs.icons, None);
        assert_eq!(tabs.count_badge, Some(false));
    }

    #[test]
    fn migrate_stamps_missing_version() {
        let out = migrate(serde_json::json!({ "theme": "dark" }));
        assert_eq!(out["schema_version"], SCHEMA_VERSION);
    }

    #[test]
    fn migrate_leaves_non_object_untouched() {
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

    #[test]
    fn parse_sets_assigns_unique_ids() {
        let value = serde_json::json!({
            "schema_version": 1,
            "sets": [
                { "name": "A" },
                { "id": 5, "name": "B" },
                { "id": 5, "name": "C" }
            ]
        });
        let sets = parse_sets(value);
        let ids: Vec<u64> = sets.sets.iter().map(|s| s.id).collect();
        assert_eq!(ids, [6, 5, 7]);
    }

    #[test]
    fn parse_sets_clamps_future_version() {
        let value = serde_json::json!({ "schema_version": 999999999999u64, "sets": [] });
        let sets = parse_sets(value);
        assert_eq!(sets.schema_version, SCHEMA_VERSION);
    }
}
