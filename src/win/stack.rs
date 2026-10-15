//! Rect math, DPI scaling, and HWND helpers shared by the hosting engine.
//!
//! Pure geometry and thin Win32 reads — no policy. Hosting decisions live in
//! `host.rs`; this module keeps coordinate/DPI care in one place (spec 01).

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::GetDpiForWindow;

/// Current display scale factor for a window (1.0 at 96 DPI). Used to map the
/// logical content rect to physical pixels per monitor (spec 02).
pub fn scale_factor(hwnd: isize) -> f32 {
    if hwnd == 0 {
        return 1.0;
    }
    let dpi = unsafe { GetDpiForWindow(HWND(hwnd as *mut _)) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}
