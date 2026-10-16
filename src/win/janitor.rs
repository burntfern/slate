//! Crash-safe restore.
//!
//! Hosting mutates a foreign window's style/ex-style and owner. If Slate is
//! force-killed those changes would stick (no taskbar button, no frame). To
//! survive that, Slate spawns a tiny watchdog of itself (`--janitor`) that
//! waits on Slate's process handle and, if Slate ever dies, replays the saved
//! styling for every hosted window. The state is handed over through a small
//! temp file that Slate rewrites on every membership change.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HWND, WAIT_EVENT};
use windows::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, IsWindow, ShowWindow, GWL_HWNDPARENT, SW_HIDE,
};

use super::host::{unregister_member, StylingState};

const MAGIC: u32 = 0x534C_4A31; // "SLJ1"
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PLACEMENT_LEN: usize =
    std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT>();

/// Controller-side handle: owns the state file and the watchdog process.
#[derive(Default)]
pub struct Janitor {
    path: Option<PathBuf>,
    started: bool,
    last: Option<Vec<u8>>,
}

impl Janitor {
    pub fn new() -> Self {
        // Recover from any prior Slate that was force-killed and whose watchdog
        // also died (tree-kill, reboot): restore its members and drop its file.
        sweep_stale();
        Self {
            path: Some(state_path()),
            started: false,
            last: None,
        }
    }

    /// Record the current styling, spawning the watchdog the first time.
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
        // Only remember the bytes once they are actually on disk, or a later
        // identical membership would be skipped and leave a stale file behind.
        if write(&path, &bytes) {
            self.last = Some(bytes);
        }
    }
}

/// Temp file the watchdog reads on Slate's death. One per Slate process.
pub fn state_path() -> PathBuf {
    std::env::temp_dir().join(format!("slate-janitor-{}.bin", std::process::id()))
}

/// Spawn the watchdog for this Slate process.
///
/// Launched through `cmd /c start` so it is NOT a child of Slate: a tree-kill
/// of Slate (`taskkill /T`, a job object, Task Manager "End task") must not take
/// the watchdog down with it, or a force-killed Slate could leave a member
/// hidden forever.
pub fn spawn(path: &Path) {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
    let _ = std::process::Command::new(comspec)
        .arg("/c")
        .arg("start")
        .arg("") // `start` treats a leading quoted arg as the window title
        .arg(&exe)
        .arg("--janitor")
        .arg(path)
        .arg(std::process::id().to_string())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

/// Restore members left by Slate processes that are no longer alive, and remove
/// their state files. Safe to run while another Slate is live (live PIDs are
/// skipped).
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
            // A recycled PID: the file is ours to overwrite, never to replay.
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

/// Parse `slate-janitor-<pid>.bin` into its PID.
fn parse_state_name(name: &str) -> Option<u32> {
    name.strip_prefix("slate-janitor-")?
        .strip_suffix(".bin")?
        .parse()
        .ok()
}

/// Whether `pid` is still running. Uses a synchronization handle so it works on
/// processes we have no other rights to.
fn process_alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => {
                // A signaled process object means the process has exited.
                let alive = WaitForSingleObject(handle, 0) != WAIT_EVENT(0);
                let _ = CloseHandle(handle);
                alive
            }
            // If the process cannot be opened we cannot confirm it is dead, so
            // assume it is alive and leave its windows alone (a live Slate must
            // never have its members stolen by the sweep).
            Err(_) => true,
        }
    }
}

/// Serialize `slate_hwnd` + the hosted windows into the watchdog's wire format.
pub fn encode(slate_hwnd: isize, members: &[(isize, StylingState)]) -> Vec<u8> {
    let mut b = Vec::with_capacity(16 + members.len() * (4 * 8 + 16 + PLACEMENT_LEN));
    b.extend_from_slice(&MAGIC.to_le_bytes());
    b.extend_from_slice(&(slate_hwnd as i64).to_le_bytes());
    b.extend_from_slice(&(members.len() as u32).to_le_bytes());
    for (hwnd, st) in members {
        b.extend_from_slice(&(*hwnd as i64).to_le_bytes());
        b.extend_from_slice(&(st.style as i64).to_le_bytes());
        b.extend_from_slice(&(st.exstyle as i64).to_le_bytes());
        let (l, t, r, bo) = st.rect;
        for v in [l, t, r, bo] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let raw = unsafe {
            std::slice::from_raw_parts(&st.placement as *const _ as *const u8, PLACEMENT_LEN)
        };
        b.extend_from_slice(raw);
    }
    b
}

/// Write the encoded state, replacing any previous file atomically. Returns
/// whether the rename onto the destination succeeded.
pub fn write(path: &Path, bytes: &[u8]) -> bool {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_err() {
        return false;
    }
    // `std::fs::rename` replaces the destination on Windows (MoveFileEx).
    std::fs::rename(&tmp, path).is_ok()
}

/// Watchdog entry point: block until `pid` exits, then restore and clean up.
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

/// Hide the watchdog's console window (it is a console binary; `start` may have
/// given it one). No-op when there is no console.
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
            // Skip a handle that was recycled by an unrelated window.
            let owner = GetWindowLongPtrW(HWND(hwnd as *mut _), GWL_HWNDPARENT);
            if owner != 0 && owner != slate_hwnd {
                continue;
            }
        }
        unregister_member(hwnd, &st);
    }
}

fn decode(bytes: &[u8]) -> Option<(isize, Vec<(isize, StylingState)>)> {
    if bytes.len() < 16 || u32::from_le_bytes(bytes[0..4].try_into().ok()?) != MAGIC {
        return None;
    }
    let slate_hwnd = i64::from_le_bytes(bytes[4..12].try_into().ok()?) as isize;
    let count = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    // Reject a bogus count before allocating: each member occupies a fixed
    // number of bytes, so the remaining file length caps how many can be present.
    const PER_MEMBER: usize = 8 + 8 + 8 + 16 + PLACEMENT_LEN;
    if count > bytes.len().saturating_sub(16) / PER_MEMBER {
        return None;
    }
    let mut off = 16;
    let mut members = Vec::with_capacity(count);
    for _ in 0..count {
        let hwnd = i64::from_le_bytes(bytes.get(off..off + 8)?.try_into().ok()?) as isize;
        off += 8;
        let style = i64::from_le_bytes(bytes.get(off..off + 8)?.try_into().ok()?) as isize;
        off += 8;
        let exstyle = i64::from_le_bytes(bytes.get(off..off + 8)?.try_into().ok()?) as isize;
        off += 8;
        let rd = |o: usize| -> Option<i32> {
            Some(i32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
        };
        let (l, t, r, bo) = (rd(off)?, rd(off + 4)?, rd(off + 8)?, rd(off + 12)?);
        off += 16;
        let mut placement: windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT =
            Default::default();
        let src = bytes.get(off..off + PLACEMENT_LEN)?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                src.as_ptr(),
                &mut placement as *mut _ as *mut u8,
                PLACEMENT_LEN,
            );
        }
        off += PLACEMENT_LEN;
        members.push((
            hwnd,
            StylingState {
                style,
                exstyle,
                rect: (l, t, r, bo),
                placement,
            },
        ));
    }
    Some((slate_hwnd, members))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT;

    fn sample() -> StylingState {
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        placement.showCmd = 3;
        placement.rcNormalPosition = RECT {
            left: 10,
            top: 20,
            right: 300,
            bottom: 400,
        };
        StylingState {
            style: 0x16CF_0000,
            exstyle: 0x100,
            rect: (10, 20, 300, 400),
            placement,
        }
    }

    #[test]
    fn round_trips_state() {
        let bytes = encode(0x1234, &[(0xABCD, sample())]);
        let (host, members) = decode(&bytes).expect("decode");
        assert_eq!(host, 0x1234);
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].0, 0xABCD);
        assert_eq!(encode(host, &members), bytes);
    }

    #[test]
    fn empty_members_round_trip() {
        let bytes = encode(0x55, &[]);
        let (host, members) = decode(&bytes).expect("decode");
        assert_eq!(host, 0x55);
        assert!(members.is_empty());
    }

    #[test]
    fn bad_magic_rejected() {
        assert!(decode(&[0u8; 16]).is_none());
    }

    #[test]
    fn absurd_count_is_rejected_without_allocating() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC.to_le_bytes());
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&bytes).is_none());
    }
}
