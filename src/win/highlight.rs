use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, SetWindowRgn, HGDIOBJ, RGN_DIFF,
    RGN_ERROR,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetWindowPos, ShowWindow,
    HWND_TOPMOST, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const BORDER: i32 = 3;
const ACCENT: COLORREF = COLORREF(0x00FF_8D4C);
const CLASS: PCWSTR = w!("Slate.PickHighlight");

static CLASS_BRUSH: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

pub struct Highlight {
    hwnd: HWND,
}

impl Highlight {
    pub fn new() -> Option<Self> {
        unsafe {
            let hinstance = GetModuleHandleW(None).ok()?;
            if CLASS_BRUSH.get().is_none() {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: HINSTANCE(hinstance.0),
                    hbrBackground: CreateSolidBrush(ACCENT),
                    lpszClassName: CLASS,
                    ..Default::default()
                };
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

    pub fn show(&self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            self.hide();
            return;
        }
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
            let outer = CreateRectRgn(0, 0, w, h);
            if outer.0.is_null() {
                return;
            }
            let inset_w = (w - BORDER).max(BORDER + 1);
            let inset_h = (h - BORDER).max(BORDER + 1);
            let inner = CreateRectRgn(BORDER, BORDER, inset_w, inset_h);
            if inner.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(outer.0));
                return;
            }
            if CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF) == RGN_ERROR {
                let _ = DeleteObject(HGDIOBJ(inner.0));
                let _ = DeleteObject(HGDIOBJ(outer.0));
                return;
            }
            let _ = DeleteObject(HGDIOBJ(inner.0));
            if SetWindowRgn(self.hwnd, Some(outer), true) == 0 {
                let _ = DeleteObject(HGDIOBJ(outer.0));
            }
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

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
