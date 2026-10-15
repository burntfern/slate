//! Top-level window discovery. Eligibility rules from spec 02:
//! visible, non-zero size, unowned, not a tool window, not no-activate,
//! not cloaked, not Slate's own process (feedback-loop guard). Elevated
//! windows are listed but flagged so the picker can disable them.

use crate::core::WindowIdentity;
use std::cell::RefCell;
use windows::core::BOOL;
use windows::core::PWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::PROCESS_NAME_WIN32;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

thread_local! {
    static FOUND: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
}

/// Enumerate eligible top-level windows with metadata.
pub fn enumerate_windows() -> Vec<WindowIdentity> {
    let our_pid = std::process::id();
    FOUND.with(|f| f.borrow_mut().clear());
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(0));
    }
    FOUND.with(|f| {
        f.borrow()
            .iter()
            .filter_map(|&hwnd| unsafe { metadata(hwnd, our_pid) })
            .collect()
    })
}

unsafe extern "system" fn enum_proc(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    if is_candidate(hwnd) {
        FOUND.with(|f| f.borrow_mut().push(hwnd));
    }
    BOOL(1)
}

/// Cheap structural eligibility test (no process queries).
unsafe fn is_candidate(hwnd: HWND) -> bool {
    if !IsWindowVisible(hwnd).as_bool() {
        return false;
    }
    // owned windows (dialogs, popups) are not group material
    if GetWindow(hwnd, GW_OWNER).is_ok() {
        return false;
    }
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TOOLWINDOW.0 != 0 || ex & WS_EX_NOACTIVATE.0 != 0 {
        return false;
    }
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return false;
    }
    if rect.right - rect.left <= 0 || rect.bottom - rect.top <= 0 {
        return false;
    }
    // cloaked UWP windows are invisible ghosts despite IsWindowVisible
    let mut cloaked: u32 = 0;
    let size = std::mem::size_of::<u32>() as u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, size).is_ok()
        && cloaked != 0
    {
        return false;
    }
    true
}

/// Current title of a window by raw handle. Empty if the handle is gone.
pub fn window_title(hwnd: isize) -> String {
    unsafe { wide_to_string(&window_text(HWND(hwnd as *mut _))) }
}

unsafe fn metadata(hwnd: HWND, our_pid: u32) -> Option<WindowIdentity> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == our_pid {
        return None; // never group Slate's own windows
    }
    let title = wide_to_string(&window_text(hwnd));
    let class = wide_to_string(&window_class(hwnd));
    let exe_path = process_path(pid).unwrap_or_default();
    let exe_name = std::path::Path::new(&exe_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| class.clone());
    Some(WindowIdentity {
        hwnd: hwnd.0 as isize,
        pid,
        exe_path,
        exe_name,
        title,
        class,
        elevated: super::integrity::is_elevated(pid),
    })
}

/// Read a window's title as a wide string.
unsafe fn window_text(hwnd: HWND) -> Vec<u16> {
    let mut buf = vec![0u16; 512];
    let len = windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf);
    if len <= 0 {
        Vec::new()
    } else {
        buf.truncate(len as usize);
        buf
    }
}

/// Read a window's class name as a wide string.
unsafe fn window_class(hwnd: HWND) -> Vec<u16> {
    let mut buf = vec![0u16; 256];
    let len = GetClassNameW(hwnd, &mut buf);
    if len <= 0 {
        Vec::new()
    } else {
        buf.truncate(len as usize);
        buf
    }
}

fn wide_to_string(buf: &[u16]) -> String {
    String::from_utf16_lossy(buf).trim().to_string()
}

/// Full executable path for a pid; None if access is denied (elevated).
fn process_path(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        buf.truncate(len as usize);
        Some(wide_to_string(&buf))
    }
}
