//! Activation with the documented workarounds: restore when minimized,
//! then SetForegroundWindow; Windows may refuse foreground grabs from a
//! background process, so we retry once via the AttachThreadInput trick
//! before giving up (spec 02).

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, GetWindowThreadProcessId, IsIconic, PostMessageW, SetForegroundWindow,
    SetWindowPos, ShowWindow, SWP_ASYNCWINDOWPOS, SWP_NOZORDER, SW_RESTORE, WM_CLOSE,
};

/// Bring the given window to the foreground. Returns true on best-effort
/// success; never panics, never touches the window beyond show/restore/focus.
pub fn activate_window(hwnd: isize) -> bool {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        if SetForegroundWindow(h).as_bool() {
            return true;
        }
        // Foreground lock workaround: attach our input threads briefly.
        let mut pid = 0u32;
        let target_thread = GetWindowThreadProcessId(h, Some(&mut pid));
        let our_thread = GetCurrentThreadId();
        if target_thread != 0 && AttachThreadInput(our_thread, target_thread, true).as_bool() {
            let ok = SetForegroundWindow(h).as_bool();
            let _ = AttachThreadInput(our_thread, target_thread, false);
            ok
        } else {
            false
        }
    }
}

/// Ask the given window to close, exactly as if the user clicked its own
/// close button: post `WM_CLOSE` and let the app run its own shutdown. The
/// window is *destroyed*, not un-hosted. Best-effort; never panics.
pub fn close_window(hwnd: isize) -> bool {
    let h = HWND(hwnd as *mut _);
    unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok() }
}

/// Move/resize a window to `(x, y, width, height)` in physical pixels. Used when
/// a tab is dragged out of the host: the window is restored, then dropped at the
/// cursor so it lands where the user released it (possibly on another monitor).
/// Best-effort; never panics, never changes z-order on its own.
pub fn move_window(hwnd: isize, x: i32, y: i32, w: i32, h: i32) -> bool {
    if hwnd == 0 {
        return false;
    }
    let win = HWND(hwnd as *mut _);
    unsafe {
        SetWindowPos(
            win,
            None,
            x,
            y,
            w.max(1),
            h.max(1),
            SWP_NOZORDER | SWP_ASYNCWINDOWPOS,
        )
        .is_ok()
    }
}

/// A window's on-screen rectangle as `(x, y, width, height)` in physical
/// pixels, or `None` if the window is gone.
///
/// Prefers DWM's *extended frame bounds*: `GetWindowRect` includes the invisible
/// resize border a window carries (several px on each side), which would leave a
/// visible gap between Slate's pick ring and the window's real edge. Falls back
/// to `GetWindowRect` when DWM has no answer (e.g. a window not yet composited).
pub fn window_rect(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    let h = HWND(hwnd as *mut _);
    let mut rect = RECT::default();
    unsafe {
        let ok = DwmGetWindowAttribute(
            h,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok();
        if !ok {
            GetWindowRect(h, &mut rect).ok()?;
        }
    }
    Some((
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
    ))
}
