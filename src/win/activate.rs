//! Activation with the documented workarounds: restore when minimized,
//! then SetForegroundWindow; Windows may refuse foreground grabs from a
//! background process, so we retry once via the AttachThreadInput trick
//! before giving up (spec 02).

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
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
        if target_thread != 0
            && AttachThreadInput(our_thread, target_thread, true).as_bool()
        {
            let ok = SetForegroundWindow(h).as_bool();
            let _ = AttachThreadInput(our_thread, target_thread, false);
            ok
        } else {
            false
        }
    }
}
