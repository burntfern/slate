use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::update::{self, parse_release_tag, LATEST_API_URL};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn fetch_latest() -> Result<String, String> {
    // No -f: the trailing status line is what the toast reports, so a 403
    // reads as a rate limit instead of curl's opaque exit 22.
    let output = Command::new("curl.exe")
        .args([
            "-sSL",
            "--max-time",
            "15",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: slate-update-check",
            "-w",
            "\n%{http_code}",
            LATEST_API_URL,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("curl: {err}"))?;
    if !output.status.success() {
        return Err(match output.status.code() {
            Some(6) => "Offline - could not reach github.com".to_string(),
            Some(28) => "Timed out - try again".to_string(),
            Some(code) => format!("Network error {code}"),
            None => "Interrupted".to_string(),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, status) = split_status(&text);
    if let Some(err) = status_error(status) {
        return Err(err);
    }
    parse_release_tag(body).ok_or_else(|| "no release tag in response".to_string())
}

/// The asset suffix `release.yml` builds for this machine.
pub fn asset_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x64",
    }
}

/// Download the release exe for `version` to the temp directory.
pub fn download_release(version: &str) -> Result<PathBuf, String> {
    let url = update::download_url(version, asset_arch());
    let dest = std::env::temp_dir().join(format!("slate-update-v{version}-{}.exe", asset_arch()));
    let dest_arg = dest.to_string_lossy().into_owned();
    let output = Command::new("curl.exe")
        .args(["-sSL", "--max-time", "300", "-f", "-o", &dest_arg, &url])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("curl: {err}"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&dest);
        return Err(match output.status.code() {
            Some(6) => "Offline - could not reach github.com".to_string(),
            Some(22) => "Release asset not ready yet".to_string(),
            Some(28) => "Timed out - try again".to_string(),
            Some(code) => format!("Network error {code}"),
            None => "Interrupted".to_string(),
        });
    }
    Ok(dest)
}

/// Windows refuses to replace a running exe, so a helper does the swap after
/// this process exits: it waits for `pid`, copies the download over `target`,
/// relaunches it, and deletes itself.
pub fn spawn_swap_helper(pid: u32, target: &Path, downloaded: &Path) -> Result<(), String> {
    let helper = std::env::temp_dir().join(format!("slate-swap-{pid}.bat"));
    std::fs::write(&helper, helper_script(pid, target, downloaded))
        .map_err(|err| format!("helper: {err}"))?;
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
    std::process::Command::new(comspec)
        .args(["/c", "start", "", "/min"])
        .arg(&helper)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|err| format!("helper: {err}"))?;
    Ok(())
}

fn helper_script(pid: u32, target: &Path, downloaded: &Path) -> String {
    format!(
        "@echo off\r\n\
         setlocal\r\n\
         set /a n=0\r\n\
         :loop\r\n\
         tasklist /fi \"PID eq {pid}\" 2>nul | find \"{pid}\" >nul\r\n\
         if errorlevel 1 goto swap\r\n\
         set /a n+=1\r\n\
         if %n% geq 600 goto swap\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         goto loop\r\n\
         :swap\r\n\
         copy /y \"{downloaded}\" \"{target}\" >nul\r\n\
         if errorlevel 1 goto fallback\r\n\
         del \"{downloaded}\" >nul\r\n\
         start \"\" \"{target}\"\r\n\
         goto cleanup\r\n\
         :fallback\r\n\
         start \"\" \"{downloaded}\"\r\n\
         :cleanup\r\n\
         del \"%~f0\"\r\n",
        pid = pid,
        target = target.display(),
        downloaded = downloaded.display()
    )
}

/// curl's `-w "\n%{http_code}"` appends the status as the last line.
fn split_status(text: &str) -> (&str, &str) {
    match text.rsplit_once('\n') {
        Some((body, status)) => (body, status.trim()),
        None => (text, ""),
    }
}

fn status_error(status: &str) -> Option<String> {
    Some(match status {
        "200" => return None,
        "403" | "429" => "GitHub is rate-limiting update checks (60/hour) - try later".to_string(),
        "404" => "No releases published yet".to_string(),
        "" | "000" => "No response from GitHub".to_string(),
        other => format!("GitHub returned HTTP {other}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{helper_script, split_status, status_error};
    use std::path::Path;

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

    #[test]
    fn swap_helper_waits_then_replaces_and_relaunches() {
        let script = helper_script(
            4242,
            Path::new("C:\\apps\\slate.exe"),
            Path::new("C:\\Temp\\slate-update.exe"),
        );
        assert!(script.contains("tasklist /fi \"PID eq 4242\""));
        assert!(script.contains("if errorlevel 1 goto swap"));
        assert!(script.contains("copy /y \"C:\\Temp\\slate-update.exe\" \"C:\\apps\\slate.exe\""));
        assert!(script.contains("start \"\" \"C:\\apps\\slate.exe\""));
        assert!(script.contains("del \"%~f0\""));
    }
}
