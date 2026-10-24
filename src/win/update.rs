use std::fs;
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use tracing::{error, info};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::config;
use crate::core::install;
use crate::core::update::{self, parse_release_tag, LATEST_API_URL};

fn http_client() -> Result<Client, String> {
    Client::builder()
        .user_agent("slate-update-check")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|err| format!("http client: {err}"))
}

pub fn bootstrap(source: &str) -> Result<(), String> {
    info!("bootstrap update from {source}");
    let source_path = Path::new(source);
    let (version, arch) = install::parse_release_filename(source_path)
        .ok_or_else(|| "downloaded exe is not a recognized release asset".to_string())?;
    let canonical = config::canonical_exe_path();
    if let Some(parent) = canonical.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("slate dir: {err}"))?;
    }
    let staging = config::staging_updates_dir();
    fs::create_dir_all(&staging).map_err(|err| format!("staging dir: {err}"))?;
    let staged = staging.join(install::STAGED_NAME);
    fs::copy(source_path, &staged).map_err(|err| format!("stage: {err}"))?;
    let asset = update::asset_name(&version, &arch);
    let sums = fetch_sums(&version)?;
    let expected = install::checksum_for(&sums, &asset)
        .ok_or_else(|| format!("missing checksum for {asset} in SHA256SUMS"))?;
    let actual = file_hash(&staged)?;
    if actual != expected {
        let _ = fs::remove_file(&staged);
        error!("checksum mismatch for {asset}");
        return Err(format!(
            "checksum mismatch for {asset}: expected {expected}, got {actual}"
        ));
    }
    let backup = install::backup_path(&canonical);
    if install::needs_backup(canonical.exists(), backup.exists()) {
        fs::rename(&canonical, &backup).map_err(|err| format!("backup: {err}"))?;
    }
    fs::rename(&staged, &canonical).map_err(|err| format!("publish: {err}"))?;
    if !config::save_install(&config::InstallRecord {
        path: canonical.to_string_lossy().into_owned(),
        version: version.clone(),
        pid: std::process::id(),
    }) {
        return Err("failed to write install record".to_string());
    }
    info!("bootstrap complete version {version}");
    Ok(())
}

pub fn refresh_install_from(candidate: &str) -> Result<(), String> {
    info!("refresh install from {candidate}");
    let candidate_path = Path::new(candidate);
    let (version, _arch) = install::parse_release_filename(candidate_path)
        .ok_or_else(|| "downloaded exe is not a recognized release asset".to_string())?;
    let canonical = config::canonical_exe_path();
    let downloaded = download_release(&version)?;
    let pid = std::process::id();
    spawn_swap_helper(pid, &canonical, &downloaded)?;
    if !config::save_install(&config::InstallRecord {
        path: canonical.to_string_lossy().into_owned(),
        version: version.clone(),
        pid,
    }) {
        return Err("failed to write install record".to_string());
    }
    info!("swap helper spawned for version {version}");
    Ok(())
}

pub fn fetch_latest() -> Result<String, String> {
    let client = http_client()?;
    let res = client
        .get(LATEST_API_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "slate-update-check")
        .send()
        .map_err(|err| {
            if err.is_timeout() {
                "Timed out - try again".to_string()
            } else if err.is_connect() {
                "Offline - could not reach github.com".to_string()
            } else {
                format!("Network error: {err}")
            }
        })?;
    let status = res.status();
    if status.as_u16() == 403 || status.as_u16() == 429 {
        return Err("GitHub is rate-limiting update checks (60/hour) - try later".to_string());
    }
    if status.as_u16() == 404 {
        return Err("No releases published yet".to_string());
    }
    if !status.is_success() {
        return Err(format!("GitHub returned HTTP {}", status.as_u16()));
    }
    let body = res.text().map_err(|err| format!("read body: {err}"))?;
    parse_release_tag(&body).ok_or_else(|| "no release tag in response".to_string())
}

pub fn asset_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x64",
    }
}

pub fn download_release(version: &str) -> Result<PathBuf, String> {
    info!("downloading release v{version}");
    let arch = asset_arch();
    let asset = update::asset_name(version, arch);
    let url = update::download_url(version, arch);
    let staging = config::staging_updates_dir();
    fs::create_dir_all(&staging).map_err(|err| format!("staging dir: {err}"))?;
    let dest = staging.join(install::staged_download_name(version, arch));
    let client = http_client()?;
    let mut res = client
        .get(&url)
        .header("User-Agent", "slate-updater")
        .send()
        .map_err(|err| {
            if err.is_timeout() {
                "Timed out - try again".to_string()
            } else if err.is_connect() {
                "Offline - could not reach github.com".to_string()
            } else {
                format!("Network error: {err}")
            }
        })?;
    if res.status().as_u16() == 404 {
        let _ = fs::remove_file(&dest);
        return Err("Release asset not ready yet".to_string());
    }
    if !res.status().is_success() {
        let _ = fs::remove_file(&dest);
        return Err(format!("Network error {}", res.status().as_u16()));
    }
    let mut file = fs::File::create(&dest).map_err(|err| format!("create file: {err}"))?;
    res.copy_to(&mut file)
        .map_err(|err| format!("download: {err}"))?;
    let _ = file.flush();
    let sums = fetch_sums(version)?;
    let expected = install::checksum_for(&sums, &asset)
        .ok_or_else(|| format!("missing checksum for {asset} in SHA256SUMS"))?;
    let actual = file_hash(&dest)?;
    if actual != expected {
        let _ = fs::remove_file(&dest);
        error!("checksum mismatch for {asset}");
        return Err(format!(
            "checksum mismatch for {asset}: expected {expected}, got {actual}"
        ));
    }
    info!("downloaded and verified {asset}");
    Ok(dest)
}

fn fetch_sums(version: &str) -> Result<String, String> {
    let sums_url = format!(
        "https://github.com/{}/releases/download/v{version}/SHA256SUMS",
        update::REPO_SLUG
    );
    let client = http_client()?;
    let res = client
        .get(&sums_url)
        .header("User-Agent", "slate-updater")
        .send()
        .map_err(|err| format!("fetch sums: {err}"))?;
    if !res.status().is_success() {
        return Err(format!(
            "failed to fetch SHA256SUMS: {}",
            res.status().as_u16()
        ));
    }
    res.text().map_err(|err| format!("read sums: {err}"))
}

fn file_hash(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|err| format!("open file: {err}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|err| format!("read file: {err}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

pub fn spawn_swap_helper(pid: u32, target: &Path, staged: &Path) -> Result<(), String> {
    let dir = config::staging_updates_dir();
    let name = swap_helper_name(pid);
    std::fs::write(dir.join(&name), helper_script(pid, target, staged))
        .map_err(|err| format!("helper: {err}"))?;
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
    Command::new(comspec)
        .current_dir(&dir)
        .args(["/c"])
        .arg(&name)
        .creation_flags(CREATE_NO_WINDOW.0)
        .spawn()
        .map_err(|err| format!("helper: {err}"))?;
    Ok(())
}

fn swap_helper_name(pid: u32) -> String {
    format!("slate-swap-{pid}.bat")
}

fn batch_arg(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('%', "%%"))
}

fn helper_script(pid: u32, target: &Path, staged: &Path) -> String {
    let backup = install::backup_path(target);
    let staged = batch_arg(staged);
    let target = batch_arg(target);
    let backup = batch_arg(&backup);
    format!(
        "@echo off\r\n\
         setlocal DisableDelayedExpansion\r\n\
         set /a n=0\r\n\
         :wait\r\n\
         tasklist /fi \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul\r\n\
         if errorlevel 1 goto swap\r\n\
         set /a n+=1\r\n\
         if %n% geq 600 goto swap\r\n\
         ping -n 2 127.0.0.1 >nul\r\n\
         goto wait\r\n\
         :swap\r\n\
         if not exist {staged} goto launch\r\n\
         if not exist {target} goto replace\r\n\
         if exist {backup} goto replace\r\n\
         move /y {target} {backup} >nul\r\n\
         :replace\r\n\
         move /y {staged} {target} >nul\r\n\
         if exist {target} goto launch\r\n\
         if exist {backup} move /y {backup} {target} >nul\r\n\
         :launch\r\n\
         if exist {target} start \"\" {target}\r\n\
         if not exist {target} if exist {staged} start \"\" {staged}\r\n\
         del \"%~f0\" >nul\r\n",
        pid = pid,
        staged = staged,
        target = target,
        backup = backup,
    )
}

#[cfg(test)]
fn split_status(text: &str) -> (&str, &str) {
    match text.rsplit_once('\n') {
        Some((body, status)) => (body, status.trim()),
        None => (text, ""),
    }
}

#[cfg(test)]
fn status_error(status: &str) -> Option<String> {
    match status {
        "200" => None,
        "403" | "429" => {
            Some("GitHub is rate-limiting update checks (60/hour) - try later".to_string())
        }
        "404" => Some("No releases published yet".to_string()),
        "" | "000" => Some("No response from GitHub".to_string()),
        other => Some(format!("GitHub returned HTTP {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_the_last_line() {
        let (body, status) = split_status("{\"tag_name\":\"v1.2.3\"}\n200");
        assert_eq!(body, "{\"tag_name\":\"v1.2.3\"}");
        assert_eq!(status, "200");

        let (body, status) = split_status("\n403");
        assert_eq!(body, "");
        assert_eq!(status, "403");
    }

    #[test]
    fn http_errors_name_the_reason() {
        assert!(status_error("200").is_none());
        assert!(status_error("403").unwrap().contains("60/hour"));
        assert!(status_error("429").unwrap().contains("rate-limit"));
        assert_eq!(status_error("500").unwrap(), "GitHub returned HTTP 500");
        assert!(status_error("").unwrap().contains("No response"));
    }

    const TARGET: &str = "C:\\Users\\a\\.burntfern\\slate\\slate.exe";
    const STAGED: &str = "C:\\Users\\a\\.burntfern\\slate\\updates\\slate-update-v0.2.0-x64.exe";

    #[test]
    fn swap_helper_waits_then_replaces_and_relaunches() {
        let script = helper_script(
            4242,
            Path::new("C:\\apps\\slate.exe"),
            Path::new("C:\\Temp\\slate-update.exe"),
        );
        assert!(script.contains("tasklist /fi \"PID eq 4242\""));
        assert!(script.contains("if errorlevel 1 goto swap"));
        assert!(script.contains("start \"\" \"C:\\apps\\slate.exe\""));
        assert!(script.contains("del \"%~f0\""));
        assert!(!script.contains("timeout"));
        assert!(script.contains("ping -n 2 127.0.0.1"));
    }

    #[test]
    fn swap_helper_backs_up_and_swaps_without_reverifying() {
        let target = Path::new(TARGET);
        let staged = Path::new(STAGED);
        let backup = install::backup_path(target);
        let script = helper_script(7, target, staged);

        assert!(!script.contains("certutil"));
        assert!(!script.contains("slate-new.exe"));
        assert!(script.contains("setlocal DisableDelayedExpansion"));

        assert!(script.contains(&format!("if not exist {} goto replace", batch_arg(target))));
        assert!(script.contains(&format!("if exist {} goto replace", batch_arg(&backup))));

        let back_up = format!("move /y {} {} >nul", batch_arg(target), batch_arg(&backup));
        let swap = format!("move /y {} {} >nul", batch_arg(staged), batch_arg(target));
        let restore = format!("move /y {} {} >nul", batch_arg(&backup), batch_arg(target));
        assert!(script.contains(&back_up));
        assert!(script.contains(&swap));
        assert!(script.contains(&restore));
        assert!(script.find(&back_up).unwrap() < script.find(&swap).unwrap());
    }

    #[test]
    fn batch_paths_stay_literal_for_cmd() {
        assert_eq!(
            batch_arg(Path::new("C:\\Users\\John Doe\\slate.exe")),
            "\"C:\\Users\\John Doe\\slate.exe\""
        );
        assert_eq!(
            batch_arg(Path::new("C:\\100%\\slate.exe")),
            "\"C:\\100%%\\slate.exe\""
        );
        assert_eq!(
            batch_arg(Path::new("C:\\a&b^c|d(e)\\slate.exe")),
            "\"C:\\a&b^c|d(e)\\slate.exe\""
        );
    }

    #[test]
    fn helper_script_escapes_special_paths() {
        let target = Path::new("C:\\Users\\a%b&c\\.burntfern\\slate\\slate.exe");
        let staged =
            Path::new("C:\\Users\\a%b&c\\.burntfern\\slate\\updates\\slate-update-v0.2.0-x64.exe");
        let script = helper_script(7, target, staged);

        assert!(!script.contains("a%b"));
        assert!(script.contains("\"C:\\Users\\a%%b&c\\.burntfern\\slate\\slate.exe\""));
        assert!(script.contains("\"C:\\Users\\a%%b&c\\.burntfern\\slate\\slate.exe.old\""));
        assert!(script.contains("\"C:\\Users\\a%%b&c\\.burntfern\\slate\\updates\\"));
    }

    #[test]
    fn swap_helper_is_handed_a_bare_name() {
        let name = swap_helper_name(4242);
        assert_eq!(name, "slate-swap-4242.bat");
        assert!(name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.')));
    }

    #[test]
    fn publish_path_is_the_canonical_slate_exe() {
        let target = config::canonical_exe_path();
        assert!(target
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("slate.exe")));
    }
}
