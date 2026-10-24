use std::path::PathBuf;

fn home_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("USERPROFILE") {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            return dir;
        }
    }
    let drive = std::env::var_os("HOMEDRIVE");
    let path = std::env::var_os("HOMEPATH");
    if let (Some(drive), Some(path)) = (drive, path) {
        let mut combined = PathBuf::from(drive);
        combined.push(path);
        if !combined.as_os_str().is_empty() {
            return combined;
        }
    }
    std::env::temp_dir()
}

fn local_app_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            return dir;
        }
    }
    home_dir().join("AppData").join("Local")
}

fn program_files_local_dir() -> PathBuf {
    local_app_data_dir().join("Programs")
}

pub fn is_portable() -> bool {
    if std::env::var_os("SLATE_PORTABLE").is_some() {
        return true;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let portable_marker = dir.join("slate.portable");
            if portable_marker.exists() {
                return true;
            }
        }
    }
    false
}

#[allow(dead_code)]
pub fn old_burntfern_root() -> PathBuf {
    home_dir().join(".burntfern")
}

#[allow(dead_code)]
pub fn old_slate_dir() -> PathBuf {
    old_burntfern_root().join("slate")
}

pub fn data_dir() -> PathBuf {
    if is_portable() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join("data");
            }
        }
    }
    local_app_data_dir().join("Burntfern").join("Slate")
}

pub fn install_dir() -> PathBuf {
    if is_portable() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.to_path_buf();
            }
        }
    }
    program_files_local_dir().join("Burntfern").join("Slate")
}

pub fn updates_dir() -> PathBuf {
    data_dir().join("updates")
}

pub fn backups_dir() -> PathBuf {
    data_dir().join("backups")
}

pub fn logs_dir() -> PathBuf {
    data_dir()
}

#[allow(dead_code)]
pub fn config_dir() -> PathBuf {
    data_dir()
}

#[allow(dead_code)]
pub fn app_dir_legacy() -> PathBuf {
    old_slate_dir()
}

pub fn ensure_dirs() {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::create_dir_all(install_dir());
    let _ = std::fs::create_dir_all(updates_dir());
    let _ = std::fs::create_dir_all(backups_dir());
    let _ = std::fs::create_dir_all(logs_dir());
}
