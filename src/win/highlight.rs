//! Pick-mode highlight ring.
//!
//! While pick mode is armed the user hovers windows and presses TAB to host
//! one, but nothing on screen says which window will be chosen. This draws a
//! thin accent ring around the window under the cursor.
//!
//! The ring is its own topmost, click-through popup window: a `WS_EX_TOPMOST`
//! window sits above an owned member (which would otherwise paint over it), and
//! `WS_EX_TRANSPARENT | WS_EX_NOACTIVATE` keeps it inert — it never takes focus
//! or intercepts the click. Only the border shows: a window region cut as a
//! hollow frame leaves the interior out of the window entirely, so the ring's
//! background brush paints nothing but the frame.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, SetWindowRgn, HGDIOBJ, RGN_DIFF,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetWindowPos, ShowWindow,
    HWND_TOPMOST, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// Ring thickness in physical pixels.
const BORDER: i32 = 3;
/// Ring colour: `#4C8DFF` as a `COLORREF` (`0x00BBGGRR`).
const ACCENT: COLORREF = COLORREF(0x00FF_8D4C);
const CLASS: PCWSTR = w!("Slate.PickHighlight");

/// Class background brush, created once and kept for the process lifetime (the
/// class references it for its whole registered life).
static CLASS_BRUSH: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// A reusable borderless ring window.
pub struct Highlight {
    hwnd: HWND,
}

impl Highlight {
    /// Create the ring window (hidden). `None` if the OS refuses the window.
    pub fn new() -> Option<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(None).ok()?;
            // Register the class once; the background brush must outlive every
            // ring window, so it is created only on first registration.
            if CLASS_BRUSH.get().is_none() {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: HINSTANCE(hinstance.0),
                    hbrBackground: CreateSolidBrush(ACCENT),
                    lpszClassName: CLASS,
                    ..Default::default()
                };
                // Ignore "class already exists": we only need it registered once.
                let _ = RegisterClassW(&wc);
                let _ = CLASS_BRUSH.set(wc.hbrBackground.0 as isize);
            }
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                CLASS,
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                None,
            )
            .ok()?;
            if hwnd.is_invalid() {
                return None;
            }
            Some(Self { hwnd })
        }
    }

    /// Move the ring over a physical-pixel rectangle and show it without
    /// activating or taking input.
    pub fn show(&self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            self.hide();
            return;
        }
        unsafe {
            // Re-assert TOPMOST every show: a fullscreen (topmost) target could
            // otherwise sit above the ring. Inserting after HWND_TOPMOST keeps
            // the ring at the top of the topmost band without activating it.
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
            // Hollow frame: outer rect minus an inset inner rect. The system
            // takes ownership of `outer` on success, so it is not deleted here.
            let outer = CreateRectRgn(0, 0, w, h);
            let inset_w = (w - BORDER).max(BORDER + 1);
            let inset_h = (h - BORDER).max(BORDER + 1);
            let inner = CreateRectRgn(BORDER, BORDER, inset_w, inset_h);
            CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
            let _ = DeleteObject(HGDIOBJ(inner.0));
            if SetWindowRgn(self.hwnd, Some(outer), true) == 0 {
                // On failure the system did not take ownership of `outer`.
                let _ = DeleteObject(HGDIOBJ(outer.0));
            }
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    /// Hide the ring.
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

impl Drop for Highlight {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
