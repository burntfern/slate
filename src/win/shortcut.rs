//! System-wide keyboard shortcuts: `Ctrl+Tab` / `Ctrl+Shift+Tab` cycle tabs,
//! `Ctrl+1..9` jump to a tab, `Ctrl+W` closes the current one.
//!
//! A hosted member is a separate top-level window, so the host's own key event
//! only fires while *it* has focus — after any tab switch the member takes focus
//! and would swallow the chords. So these are read from the same low-level
//! keyboard hook pick mode uses, installed once and left running. A chord is
//! only ever swallowed while a Slate-owned window (the host or one of its
//! members) is in the foreground, so every other app keeps its own `Ctrl`
//! chords untouched.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_1, VK_9, VK_CONTROL, VK_SHIFT, VK_TAB, VK_W,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// A chord the hook recognised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    NextTab,
    PrevTab,
    CloseTab,
    /// `Ctrl+N` — jump to the Nth tab.
    SelectIndex(u8),
}

fn queue() -> &'static Mutex<Vec<Shortcut>> {
    static QUEUE: OnceLock<Mutex<Vec<Shortcut>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

fn owned() -> &'static Mutex<HashSet<isize>> {
    static OWNED: OnceLock<Mutex<HashSet<isize>>> = OnceLock::new();
    OWNED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The windows that count as "Slate is focused": the host plus every member.
/// Updated on each layout sync; a chord is only consumed while the foreground
/// window is one of these.
pub fn set_owned(hwnds: impl IntoIterator<Item = isize>) {
    if let Ok(mut set) = owned().lock() {
        set.clear();
        set.extend(hwnds);
    }
}

fn held(key: VIRTUAL_KEY) -> bool {
    // Safety: `GetAsyncKeyState` is a pure query of the current key state.
    unsafe { GetAsyncKeyState(key.0 as i32) < 0 }
}

/// Whether Ctrl is held right now. Used by the Ctrl-drag "add window" gesture.
pub fn ctrl_held() -> bool {
    held(VK_CONTROL)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let message = wparam.0 as u32;
        if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
            // Safety: the OS passes a KBDLLHOOKSTRUCT for WH_KEYBOARD_LL.
            let keyboard = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            // Alt+Tab and friends belong to Windows, not us.
            let alt_held = keyboard.flags.0 & LLKHF_ALTDOWN.0 != 0;
            if held(VK_CONTROL) && !alt_held {
                let vk = keyboard.vkCode as u16;
                let shortcut = match vk {
                    v if v == VK_TAB.0 => Some(if held(VK_SHIFT) {
                        Shortcut::PrevTab
                    } else {
                        Shortcut::NextTab
                    }),
                    v if v == VK_W.0 => Some(Shortcut::CloseTab),
                    v if (VK_1.0..=VK_9.0).contains(&v) => {
                        Some(Shortcut::SelectIndex((v - VK_1.0 + 1) as u8))
                    }
                    _ => None,
                };
                if let Some(shortcut) = shortcut {
                    // Safety: `GetForegroundWindow` is a pure query.
                    let foreground = unsafe { GetForegroundWindow() }.0 as isize;
                    let ours = owned()
                        .lock()
                        .map(|set| set.contains(&foreground))
                        .unwrap_or(false);
                    if ours {
                        if let Ok(mut queue) = queue().lock() {
                            queue.push(shortcut);
                        }
                        // Swallow it so the member never sees the chord.
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    // Safety: forwarding to the rest of the hook chain.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The persistent shortcut hook. Unlike pick mode's capture this lives for the
/// whole run; dropping it (on shutdown) unhooks it.
pub struct ShortcutCapture {
    hook: HHOOK,
}

impl ShortcutCapture {
    /// Install the hook. Returns `None` when the OS refuses; Slate then simply
    /// has no global shortcuts.
    pub fn install() -> Option<Self> {
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) }.ok()?;
        Some(Self { hook })
    }

    /// Take the chords recognised since the last drain.
    pub fn drain(&self) -> Vec<Shortcut> {
        queue()
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

impl Drop for ShortcutCapture {
    fn drop(&mut self) {
        // Safety: `hook` came from SetWindowsHookExW and is unhooked once here.
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
        }
    }
}
