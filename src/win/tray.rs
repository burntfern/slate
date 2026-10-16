//! System-tray icon (no onboarding).
//!
//! A hidden window owns a `Shell_NotifyIcon` icon; the host's own message pump
//! dispatches its messages (same thread, so no extra message loop). Left-click
//! restores the host; right-click shows a small menu (Show Slate / Exit). The
//! tray only ever *restores* or *requests close* of the host it was handed via
//! [`TrayIcon::set_host`]; it never shows/hides members or steals focus.

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, GetCursorPos, PostMessageW,
    RegisterClassW, SetForegroundWindow, ShowWindow, TrackPopupMenu, HICON, MF_STRING, SW_RESTORE,
    TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP,
    WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

use super::icon;

/// Icon id and the callback message the shell posts to our hidden window.
const TRAY_UID: u32 = 1;
const TRAY_MSG: u32 = WM_APP + 1;
/// Context-menu command ids.
const CMD_SHOW: i32 = 1;
const CMD_EXIT: i32 = 2;

/// The host window to restore / close. Updated on each `HostReady`.
static HOST: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn notify_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    // Safety: a zeroed NOTIFYICONDATAW is a valid "empty" record; every field we
    // care about is set below.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = TRAY_UID;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = TRAY_MSG;
    nid.hIcon = icon;
    let tip = to_wide("Slate");
    let n = tip.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    nid
}

/// Bring the host window back and focus it.
fn restore_host() {
    let host = HOST.load(std::sync::atomic::Ordering::Relaxed);
    if host == 0 {
        return;
    }
    unsafe {
        let h = HWND(host as *mut _);
        let _ = ShowWindow(h, SW_RESTORE);
        let _ = SetForegroundWindow(h);
    }
}

/// Right-click menu: Show Slate / Exit.
unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let menu = match windows::Win32::UI::WindowsAndMessaging::CreatePopupMenu() {
            Ok(menu) => menu,
            Err(_) => return,
        };
        let show = to_wide("Show Slate");
        let exit = to_wide("Exit");
        let _ = windows::Win32::UI::WindowsAndMessaging::AppendMenuW(
            menu,
            MF_STRING,
            CMD_SHOW as usize,
            windows::core::PCWSTR(show.as_ptr()),
        );
        let _ = windows::Win32::UI::WindowsAndMessaging::AppendMenuW(
            menu,
            MF_STRING,
            CMD_EXIT as usize,
            windows::core::PCWSTR(exit.as_ptr()),
        );
        let mut pt = windows::Win32::Foundation::POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu dismisses correctly when clicking elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        match cmd.0 {
            CMD_SHOW => restore_host(),
            CMD_EXIT => {
                let host = HOST.load(std::sync::atomic::Ordering::Relaxed);
                if host != 0 {
                    let _ =
                        PostMessageW(Some(HWND(host as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        TRAY_MSG => {
            match (lparam.0 & 0xFFFF) as u32 {
                WM_LBUTTONUP => restore_host(),
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let nid = notify_data(hwnd, HICON(std::ptr::null_mut()));
            // Safety: NIM_DELETE on a record we added is fine.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// The live tray icon. Dropping it removes the icon and destroys its window.
pub struct TrayIcon {
    hwnd: HWND,
    icon: HICON,
}

impl TrayIcon {
    /// Create the hidden window and add the icon. Returns `None` when the OS
    /// refuses (Slate then simply has no tray presence).
    pub fn start() -> Option<Self> {
        unsafe {
            let module = GetModuleHandleW(None).ok()?;
            let class = to_wide("SlateTrayWnd");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: HINSTANCE(module.0),
                lpszClassName: windows::core::PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            // Ignore "already registered": only one tray per process.
            let _ = RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::PCWSTR(class.as_ptr()),
                windows::core::PCWSTR(class.as_ptr()),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(HINSTANCE(module.0)),
                None,
            )
            .ok()?;
            let icon = icon::app_hicon(32)
                .map(|p| HICON(p as *mut _))
                .unwrap_or(HICON(std::ptr::null_mut()));
            let nid = notify_data(hwnd, icon);
            if Shell_NotifyIconW(NIM_ADD, &nid).0 == 0 {
                let _ = DestroyWindow(hwnd);
                return None;
            }
            Some(Self { hwnd, icon })
        }
    }

    /// Record the host window so left-click can restore it and Exit can close it.
    pub fn set_host(&self, hwnd: isize) {
        HOST.store(hwnd, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let nid = notify_data(self.hwnd, self.icon);
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
