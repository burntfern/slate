use std::ffi::c_void;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW,
    SetForegroundWindow, SetWindowPos, ShowWindow, SWP_ASYNCWINDOWPOS, SWP_NOZORDER, SW_RESTORE,
    WM_CLOSE,
};

/// Bring any visible top-level window of `pid` to the front; used when a
/// second copy of Slate finds one already running.
pub fn activate_process(pid: u32) -> bool {
    struct Search {
        pid: u32,
        found: isize,
    }
    unsafe extern "system" fn probe(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut Search);
        let mut window_pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
        if window_pid == search.pid && IsWindowVisible(hwnd).as_bool() {
            search.found = hwnd.0 as isize;
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut search = Search { pid, found: 0 };
    unsafe {
        let _ = EnumWindows(Some(probe), LPARAM(std::ptr::addr_of_mut!(search) as isize));
    }
    if search.found != 0 {
        activate_window(search.found)
    } else {
        false
    }
}

pub fn activate_window(hwnd: isize) -> bool {
    let h = HWND(hwnd as *mut _);
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        if SetForegroundWindow(h).as_bool() {
            return true;
        }
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

pub fn close_window(hwnd: isize) -> bool {
    let h = HWND(hwnd as *mut _);
    unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok() }
}

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
