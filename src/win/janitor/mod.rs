mod wire;

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HWND, WAIT_EVENT};
use windows::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, IsWindow, ShowWindow, GWL_HWNDPARENT, SW_HIDE,
};

use super::host::{unregister_member, StylingState};
pub use wire::{decode, encode};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Default)]
pub struct Janitor {
    path: Option<PathBuf>,
    started: bool,
    last: Option<Vec<u8>>,
}

impl Janitor {
    pub fn new() -> Self {
        sweep_stale();
        Self {
            path: Some(state_path()),
            started: false,
            last: None,
        }
    }

    pub fn persist(&mut self, slate_hwnd: isize, members: &[(isize, StylingState)]) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let bytes = encode(slate_hwnd, members);
        if self.last.as_deref() == Some(bytes.as_slice()) {
            return;
        }
        if !self.started {
            spawn(&path);
            self.started = true;
        }
        if write(&path, &bytes) {
            self.last = Some(bytes);
        }
    }
}

pub fn state_path() -> PathBuf {
    std::env::temp_dir().join(format!("slate-janitor-{}.bin", std::process::id()))
}

pub fn spawn(path: &Path) {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
    let _ = std::process::Command::new(comspec)
        .arg("/c")
        .arg("start")
        .arg("")
        .arg(&exe)
        .arg("--janitor")
        .arg(path)
        .arg(std::process::id().to_string())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

pub fn sweep_stale() {
    let dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let me = std::process::id();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = parse_state_name(&name.to_string_lossy()) else {
            continue;
        };
        if pid == me {
            let _ = std::fs::remove_file(entry.path());
            continue;
        }
        if process_alive(pid) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(entry.path()) {
            restore(&bytes);
        }
        let _ = std::fs::remove_file(entry.path());
    }
}

fn parse_state_name(name: &str) -> Option<u32> {
    name.strip_prefix("slate-janitor-")?
        .strip_suffix(".bin")?
        .parse()
        .ok()
}

fn process_alive(pid: u32) -> bool {
    // ERROR_INVALID_PARAMETER (87) means the pid is gone; any other error (e.g. access denied for an elevated process) counts as alive.
    const ERROR_INVALID_PARAMETER_LOW: u32 = 87;
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => {
                let alive = WaitForSingleObject(handle, 0) != WAIT_EVENT(0);
                let _ = CloseHandle(handle);
                alive
            }
            Err(e) => (e.code().0 as u32 & 0xFFFF) != ERROR_INVALID_PARAMETER_LOW,
        }
    }
}

pub fn write(path: &Path, bytes: &[u8]) -> bool {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_err() {
        return false;
    }
    std::fs::rename(&tmp, path).is_ok()
}

pub fn run(path: PathBuf, pid: u32) {
    if pid == 0 {
        return;
    }
    hide_console();
    let _ = watch(pid);
    if let Ok(bytes) = std::fs::read(&path) {
        restore(&bytes);
    }
    let _ = std::fs::remove_file(&path);
}

fn hide_console() {
    use windows::Win32::System::Console::GetConsoleWindow;
    unsafe {
        let console = GetConsoleWindow();
        if !console.0.is_null() {
            let _ = ShowWindow(console, SW_HIDE);
        }
    }
}

fn watch(pid: u32) -> windows::core::Result<()> {
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, false, pid)?;
        WaitForSingleObject(handle, INFINITE);
        let _ = CloseHandle(handle);
    }
    Ok(())
}

fn restore(bytes: &[u8]) {
    let Some((slate_hwnd, members)) = decode(bytes) else {
        return;
    };
    for (hwnd, st) in members {
        unsafe {
            if !IsWindow(Some(HWND(hwnd as *mut _))).as_bool() {
                continue;
            }
            let owner = GetWindowLongPtrW(HWND(hwnd as *mut _), GWL_HWNDPARENT);
            if owner != 0 && owner != slate_hwnd {
                continue;
            }
        }
        unregister_member(hwnd, &st);
    }
}
