//! Elevation check: a process running elevated holds a full token.
//! Used to disable grouping of admin windows in the picker (spec 02).

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// True if the process owning this pid runs at a higher integrity level
/// (admin). Slate never attempts to control such windows.
pub fn is_elevated(pid: u32) -> bool {
    unsafe { elevated_inner(pid).unwrap_or(false) }
}

unsafe fn elevated_inner(pid: u32) -> Option<bool> {
    let process: HANDLE =
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut token = HANDLE::default();
    let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token);
    let _ = CloseHandle(process);
    if opened.is_err() {
        return None;
    }
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    let ok = GetTokenInformation(
        token,
        TokenElevation,
        Some(&mut elevation as *mut _ as *mut _),
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut returned,
    );
    let _ = CloseHandle(token);
    if ok.is_ok() {
        Some(elevation.TokenIsElevated != 0)
    } else {
        None
    }
}
