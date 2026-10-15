//! Pick mode plumbing: which window is under the cursor, and a system-wide
//! keyboard capture for TAB/Esc while the user hovers another app (spec 02).
//!
//! The low-level keyboard hook fires on the thread that installed it, during
//! that thread's message pump, so install and drop both happen on the UI
//! thread that owns the host window.

use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_TAB};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetCursorPos, SetWindowsHookExW, UnhookWindowsHookEx,
    WindowFromPoint, GA_ROOT, HHOOK, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_SYSKEYDOWN,
};

/// A key pick mode listens for while it is armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKey {
    Tab,
    Escape,
}

fn queue() -> &'static Mutex<Vec<PickKey>> {
    static QUEUE: OnceLock<Mutex<Vec<PickKey>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
            // Safety: the OS passes a KBDLLHOOKSTRUCT for WH_KEYBOARD_LL.
            let keyboard = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            // Alt+Tab and friends are not ours; only a plain TAB or Esc is.
            let alt_held = keyboard.flags.0 & LLKHF_ALTDOWN.0 != 0;
            if !alt_held {
                let key = keyboard.vkCode as u16;
                if key == VK_TAB.0 {
                    if let Ok(mut queue) = queue().lock() {
                        queue.push(PickKey::Tab);
                    }
                    return LRESULT(1);
                }
                if key == VK_ESCAPE.0 {
                    if let Ok(mut queue) = queue().lock() {
                        queue.push(PickKey::Escape);
                    }
                    return LRESULT(1);
                }
            }
        }
    }
    // Safety: hand the event to the next hook in the chain.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Owns a system-wide low-level keyboard hook for the life of one pick.
/// Dropping it unhooks, so a pick can never leave a hook behind (spec 06).
pub struct KeyboardCapture {
    hook: HHOOK,
}

impl KeyboardCapture {
    /// Install the capture. Returns None if the OS refused the hook; pick mode
    /// then still works while the host window holds focus.
    pub fn install() -> Option<Self> {
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) }.ok()?;
        Some(Self { hook })
    }

    /// Take the keys captured since the last drain.
    pub fn drain(&self) -> Vec<PickKey> {
        queue()
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

impl Drop for KeyboardCapture {
    fn drop(&mut self) {
        // Safety: `hook` came from SetWindowsHookExW and is unhooked once here.
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
        }
    }
}

/// The top-level window under the cursor, folded up from whatever child the
/// pointer is over, or None when there is none.
pub fn window_under_cursor() -> Option<isize> {
    let mut point = POINT::default();
    // Safety: `point` is a valid, writable POINT; both calls are read-only.
    unsafe {
        GetCursorPos(&mut point).ok()?;
        let hwnd = WindowFromPoint(point);
        if hwnd.0.is_null() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        (!root.0.is_null()).then_some(root.0 as isize)
    }
}
