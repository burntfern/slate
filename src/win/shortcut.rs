use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_1, VK_9, VK_CONTROL, VK_SHIFT, VK_TAB, VK_W,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    NextTab,
    PrevTab,
    CloseTab,
    SelectIndex(u8),
}

fn queue() -> &'static Mutex<Vec<Shortcut>> {
    static QUEUE: OnceLock<Mutex<Vec<Shortcut>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

const MAX_OWNED: usize = 64;

struct OwnedSet {
    len: AtomicUsize,
    items: [AtomicIsize; MAX_OWNED],
}

impl OwnedSet {
    const fn new() -> Self {
        Self {
            len: AtomicUsize::new(0),
            items: [const { AtomicIsize::new(0) }; MAX_OWNED],
        }
    }

    fn set(&self, hwnds: impl IntoIterator<Item = isize>) {
        let mut n = 0;
        for hwnd in hwnds {
            if n == MAX_OWNED {
                break;
            }
            self.items[n].store(hwnd, Ordering::Relaxed);
            n += 1;
        }
        self.len.store(n, Ordering::Release);
    }

    fn contains(&self, hwnd: isize) -> bool {
        let n = self.len.load(Ordering::Acquire).min(MAX_OWNED);
        (0..n).any(|i| self.items[i].load(Ordering::Relaxed) == hwnd)
    }
}

fn owned() -> &'static OwnedSet {
    static OWNED: OwnedSet = OwnedSet::new();
    &OWNED
}

pub fn set_owned(hwnds: impl IntoIterator<Item = isize>) {
    owned().set(hwnds);
}

fn held(key: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(key.0 as i32) < 0 }
}

pub fn ctrl_held() -> bool {
    held(VK_CONTROL)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
            let keyboard = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let vk = keyboard.vkCode as u16;
            let chord = vk == VK_TAB.0 || vk == VK_W.0 || (VK_1.0..=VK_9.0).contains(&vk);
            if chord {
                let alt_held = keyboard.flags.0 & LLKHF_ALTDOWN.0 != 0;
                if !alt_held && held(VK_CONTROL) {
                    let shortcut = match vk {
                        v if v == VK_TAB.0 => {
                            if held(VK_SHIFT) {
                                Shortcut::PrevTab
                            } else {
                                Shortcut::NextTab
                            }
                        }
                        v if v == VK_W.0 => Shortcut::CloseTab,
                        v => Shortcut::SelectIndex((v - VK_1.0 + 1) as u8),
                    };
                    let foreground = unsafe { GetForegroundWindow() }.0 as isize;
                    if owned().contains(foreground) {
                        if let Ok(mut queue) = queue().lock() {
                            queue.push(shortcut);
                        }
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

pub struct ShortcutCapture {
    hook: HHOOK,
}

impl ShortcutCapture {
    pub fn install() -> Option<Self> {
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) }.ok()?;
        Some(Self { hook })
    }

    pub fn drain(&self) -> Vec<Shortcut> {
        queue()
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

impl Drop for ShortcutCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
        }
    }
}
