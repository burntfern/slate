use std::os::windows::process::CommandExt;
use std::process::Command;

use crate::core::update::{parse_release_tag, LATEST_API_URL};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn fetch_latest() -> Result<String, String> {
    let output = Command::new("curl.exe")
        .args([
            "-fsSL",
            "--max-time",
            "15",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: slate-update-check",
            LATEST_API_URL,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("curl: {err}"))?;
    if !output.status.success() {
        return Err(format!("curl exited {:?}", output.status.code()));
    }
    let body = String::from_utf8_lossy(&output.stdout);
    parse_release_tag(&body).ok_or_else(|| "no release tag in response".to_string())
}
