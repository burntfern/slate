use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_TAB};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetCursorPos, SetWindowsHookExW, UnhookWindowsHookEx,
    WindowFromPoint, GA_ROOT, HHOOK, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_SYSKEYDOWN,
};

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
            let keyboard = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
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
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub struct KeyboardCapture {
    hook: HHOOK,
}

impl KeyboardCapture {
    pub fn install() -> Option<Self> {
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) }.ok()?;
        Some(Self { hook })
    }

    pub fn drain(&self) -> Vec<PickKey> {
        queue()
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

impl Drop for KeyboardCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
        }
    }
}

pub fn window_under_cursor() -> Option<isize> {
    let mut point = POINT::default();
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
