//! WinEvent hooks: the OS → Slate notification channel. WINEVENT_OUTOFCONTEXT
//! delivers callbacks on the thread that installed the hook, so install from
//! the UI thread and events arrive during its message pump. Events land in a
//! thread-local queue that the app drains once per frame — callbacks do no
//! work beyond pushing (spec 02).

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

/// A lifecycle change Slate cares about. hwnd is an isize so core never
/// touches the windows types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinEvent {
    Destroyed(isize),
    TitleChanged(isize),
    ForegroundChanged(isize),
}

thread_local! {
    static QUEUE: std::cell::RefCell<Vec<WinEvent>> = const { std::cell::RefCell::new(Vec::new()) };
}

unsafe extern "system" fn hook_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    idobject: i32,
    idchild: i32,
    _thread: u32,
    _time: u32,
) {
    // Only top-level window objects: ignore per-child notifications.
    if idobject != OBJID_WINDOW.0 || idchild != 0 {
        return;
    }
    if hwnd.0.is_null() {
        return;
    }
    let e = match event {
        EVENT_OBJECT_DESTROY => WinEvent::Destroyed(hwnd.0 as isize),
        EVENT_OBJECT_NAMECHANGE => WinEvent::TitleChanged(hwnd.0 as isize),
        EVENT_SYSTEM_FOREGROUND => WinEvent::ForegroundChanged(hwnd.0 as isize),
        _ => return,
    };
    QUEUE.with(|q| q.borrow_mut().push(e));
}

/// Owns the installed hooks. Dropping unhooks everything.
pub struct WinEventPump {
    hooks: Vec<HWINEVENTHOOK>,
}

impl WinEventPump {
    /// Install hooks for destroy, title-change, and foreground events.
    /// Returns None if the OS refused a hook (rare; app keeps running
    /// without live updates — polling in the picker still works).
    pub fn install() -> Option<Self> {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let mut hooks = Vec::new();
        unsafe {
            // One single-event hook per event we act on. A broad
            // `destroy..=namechange` range would fire the callback for every
            // window in the system on dozens of unrelated event types; the
            // narrow ranges keep the callbacks to the two we care about.
            for event in [EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE] {
                let hook = SetWinEventHook(event, event, None, Some(hook_proc), 0, 0, flags);
                if hook.0.is_null() {
                    for h in hooks.drain(..) {
                        let _ = UnhookWinEvent(h);
                    }
                    return None;
                }
                hooks.push(hook);
            }
            let fg = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(hook_proc),
                0,
                0,
                flags,
            );
            if !fg.0.is_null() {
                hooks.push(fg);
            }
        }
        Some(Self { hooks })
    }

    /// Take everything queued since the last drain. Called from the UI
    /// thread only.
    pub fn drain() -> Vec<WinEvent> {
        QUEUE.with(|q| q.borrow_mut().drain(..).collect())
    }
}

impl Drop for WinEventPump {
    fn drop(&mut self) {
        for h in self.hooks.drain(..) {
            unsafe {
                let _ = UnhookWinEvent(h);
            }
        }
    }
}
