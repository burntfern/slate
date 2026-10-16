use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinEvent {
    Destroyed(isize),
    TitleChanged(isize),
    ForegroundChanged(isize),
    MoveStarted(isize),
    MoveEnded(isize),
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
        EVENT_SYSTEM_MOVESIZESTART => WinEvent::MoveStarted(hwnd.0 as isize),
        EVENT_SYSTEM_MOVESIZEEND => WinEvent::MoveEnded(hwnd.0 as isize),
        _ => return,
    };
    QUEUE.with(|q| q.borrow_mut().push(e));
}

pub struct WinEventPump {
    hooks: Vec<HWINEVENTHOOK>,
}

impl WinEventPump {
    pub fn install() -> Option<Self> {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let mut hooks = Vec::new();
        unsafe {
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
            let move_start = SetWinEventHook(
                EVENT_SYSTEM_MOVESIZESTART,
                EVENT_SYSTEM_MOVESIZESTART,
                None,
                Some(hook_proc),
                0,
                0,
                flags,
            );
            if !move_start.0.is_null() {
                hooks.push(move_start);
            }
            let move_end = SetWinEventHook(
                EVENT_SYSTEM_MOVESIZEEND,
                EVENT_SYSTEM_MOVESIZEEND,
                None,
                Some(hook_proc),
                0,
                0,
                flags,
            );
            if !move_end.0.is_null() {
                hooks.push(move_end);
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
