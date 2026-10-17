use std::path::{Path, PathBuf};

use super::load::app_dir;
use super::save::atomic_write;
use super::schema::InstallRecord;

pub fn install_path() -> PathBuf {
    app_dir().join("install.json")
}

pub fn load_install() -> Option<InstallRecord> {
    read_install(&install_path())
}

pub fn save_install(record: &InstallRecord) -> bool {
    write_install(&install_path(), record)
}

fn read_install(path: &Path) -> Option<InstallRecord> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_install(path: &Path, record: &InstallRecord) -> bool {
    let Ok(mut text) = serde_json::to_string_pretty(record) else {
        return false;
    };
    text.push('\n');
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    atomic_write(path, text.as_bytes()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("slate-install-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn missing_file_reads_as_no_record() {
        assert_eq!(read_install(&scratch("absent.json")), None);
    }

    #[test]
    fn record_round_trips_through_disk() {
        let path = scratch("roundtrip.json");
        let record = InstallRecord {
            path: "C:\\apps\\slate.exe".into(),
            version: "0.1.1".into(),
            pid: 99,
        };
        assert!(write_install(&path, &record));
        assert_eq!(read_install(&path), Some(record));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn corrupt_file_reads_as_no_record() {
        let path = scratch("corrupt.json");
        std::fs::write(&path, "not json at all").unwrap();
        assert_eq!(read_install(&path), None);
        let _ = std::fs::remove_file(path);
    }
}
