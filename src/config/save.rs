use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::load::{app_dir, sets_path, settings_path};
use super::schema::{SavedSets, Settings};

/// Persist settings atomically, creating the directory if needed.
pub fn save(settings: &Settings) {
    let _ = std::fs::create_dir_all(app_dir());
    let Ok(mut json) = serde_json::to_string_pretty(settings) else {
        return;
    };
    json.push('\n');
    atomic_write(&settings_path(), json.as_bytes());
}

/// Persist the saved tab sets atomically, creating the directory if needed.
pub fn save_sets(sets: &SavedSets) {
    let _ = std::fs::create_dir_all(app_dir());
    let Ok(mut json) = serde_json::to_string_pretty(sets) else {
        return;
    };
    json.push('\n');
    atomic_write(&sets_path(), json.as_bytes());
}

/// A unique sibling temp path, so concurrent saves never clobber a shared `.tmp`.
fn temp_path_for(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

/// Write `bytes` to `path` via a flushed temp file + rename so a crash never
/// leaves a truncated file in place of the last good one.
fn atomic_write(path: &Path, bytes: &[u8]) -> bool {
    let tmp = temp_path_for(path);
    let flushed = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if flushed.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}
