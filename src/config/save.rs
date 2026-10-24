use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};

use super::load::{sets_path, settings_path, shared_config_path};
use super::paths;
use super::schema::{AppSettings, SavedSets, Settings, SCHEMA_VERSION};

pub(crate) fn app_value(settings: &Settings) -> Value {
    serde_json::to_value(AppSettings {
        schema_version: SCHEMA_VERSION,
        tabs: &settings.tabs,
        updates: &settings.updates,
    })
    .unwrap_or_else(|_| json!({ "schema_version": SCHEMA_VERSION }))
}

pub fn save(settings: &Settings) -> io::Result<()> {
    std::fs::create_dir_all(paths::data_dir())?;
    let value = app_value(settings);
    let mut text = serde_json::to_string_pretty(&value).map_err(io::Error::other)?;
    text.push('\n');
    atomic_write(&settings_path(), text.as_bytes())
}

pub fn save_sets(sets: &SavedSets) -> io::Result<()> {
    std::fs::create_dir_all(paths::data_dir())?;
    let mut text = serde_json::to_string_pretty(sets).map_err(io::Error::other)?;
    text.push('\n');
    atomic_write(&sets_path(), text.as_bytes())
}

pub fn save_theme(theme: &str, accent: &str) -> io::Result<()> {
    let path = shared_config_path();
    let mut value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("schema_version".into(), json!(SCHEMA_VERSION));
        object.insert("theme".into(), json!(theme));
        object.insert("accent".into(), json!(accent));
    }
    let mut text = serde_json::to_string_pretty(&value).map_err(io::Error::other)?;
    text.push('\n');
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    atomic_write(&path, text.as_bytes())
}

fn temp_path_for(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temp_path_for(path);
    let written = (|| -> io::Result<()> {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(err) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err)
        }
    }
}
