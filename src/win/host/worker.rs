use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetForegroundWindow, GetWindowLongPtrW, IsHungAppWindow, IsIconic,
    IsWindowVisible, SetWindowPos, ShowWindow, GWLP_WNDPROC, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNA, WM_MOVE, WM_SIZE, WM_WINDOWPOSCHANGED, WNDPROC,
};

use super::geometry::{content_rect_screen, member_insets, place_member};
use super::lifecycle::set_long_ptr;

#[derive(Clone, Debug)]
struct HostShared {
    host: isize,
    titlebar: f32,
    active: Option<isize>,
    hidden: Vec<isize>,
    inset: (i32, i32, i32, i32),
}

impl Default for HostShared {
    fn default() -> Self {
        Self {
            host: 0,
            titlebar: 0.0,
            active: None,
            hidden: Vec::new(),
            inset: (0, 0, 0, 0),
        }
    }
}

#[derive(Clone)]
pub struct HostWorker {
    shared: Arc<Mutex<HostShared>>,
    running: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    wake: Arc<(Mutex<()>, Condvar)>,
}

impl Default for HostWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl HostWorker {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(HostShared::default())),
            running: Arc::new(AtomicBool::new(true)),
            started: Arc::new(AtomicBool::new(false)),
            wake: Arc::new((Mutex::new(()), Condvar::new())),
        }
    }

    pub fn update(&self, host: isize, titlebar: f32, active: Option<isize>, hidden: Vec<isize>) {
        {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            s.host = host;
            s.titlebar = titlebar.max(0.0);
            s.active = active;
            s.hidden = hidden;
        }
        self.wake.1.notify_all();
    }

    pub fn start(&self) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.shared.clone();
        let running = self.running.clone();
        let wake = self.wake.clone();
        thread::spawn(move || {
            let mut last = WorkerState::default();
            while running.load(Ordering::Relaxed) {
                {
                    let (lock, cvar) = &*wake;
                    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = cvar
                        .wait_timeout(guard, Duration::from_millis(50))
                        .unwrap_or_else(|e| e.into_inner());
                }
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                let snap = {
                    let s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    Snapshot {
                        host: s.host,
                        titlebar: s.titlebar,
                        active: s.active,
                        hidden: s.hidden.clone(),
                    }
                };
                apply(snap, &mut last);
                {
                    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    s.inset = last.inset;
                }
            }
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.wake.1.notify_all();
    }

    pub fn install_move_tracker(&self, host: isize) {
        if host == 0 || TRACKER_ORIG.with(|o| o.get()) != 0 {
            return;
        }
        let hwnd = HWND(host as *mut _);
        unsafe {
            let orig = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
            if orig == 0 {
                return;
            }
            if !set_long_ptr(hwnd, GWLP_WNDPROC, host_proc as *const () as isize) {
                return;
            }
            TRACKER_ORIG.with(|o| o.set(orig));
            TRACKER.with(|t| *t.borrow_mut() = Some(self.shared.clone()));
        }
    }
}

impl Drop for HostWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

thread_local! {
    static TRACKER: RefCell<Option<Arc<Mutex<HostShared>>>> = const { RefCell::new(None) };
    static TRACKER_ORIG: Cell<isize> = const { Cell::new(0) };
}

unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let orig = TRACKER_ORIG.with(|o| o.get());
    let result = CallWindowProcW(std::mem::transmute::<isize, WNDPROC>(orig), hwnd, msg, w, l);
    if msg == WM_WINDOWPOSCHANGED || msg == WM_MOVE || msg == WM_SIZE {
        retrack(msg == WM_SIZE);
    }
    result
}

fn retrack(resize: bool) {
    let snap = TRACKER.with(|t| {
        t.borrow().as_ref().map(|s| {
            let g = s.lock().unwrap_or_else(|e| e.into_inner());
            (g.host, g.titlebar, g.active, g.hidden.clone(), g.inset)
        })
    });
    let Some((host, titlebar, active, hidden, inset)) = snap else {
        return;
    };
    let Some(a) = active else {
        return;
    };
    if hidden.contains(&a) {
        return;
    }
    let Some(content) = content_rect_screen(host, titlebar) else {
        return;
    };
    let active = HWND(a as *mut _);
    let inset = member_insets(active).unwrap_or(inset);
    let posted = unsafe { IsHungAppWindow(active).as_bool() };
    place_member(active, content, inset, resize, posted);
}

#[derive(Clone, Default)]
struct Snapshot {
    host: isize,
    titlebar: f32,
    active: Option<isize>,
    hidden: Vec<isize>,
}

#[derive(Default)]
struct WorkerState {
    active: Option<isize>,
    hidden: Vec<isize>,
    rect: Option<(i32, i32, i32, i32)>,
    inset: (i32, i32, i32, i32),
    settle: u8,
}

const SETTLE_PASSES: u8 = 12;

fn apply(mut s: Snapshot, last: &mut WorkerState) {
    let host = HWND(s.host as *mut _);
    if host.0.is_null() {
        return;
    }
    unsafe {
        if IsIconic(host).as_bool() || !IsWindowVisible(host).as_bool() {
            return;
        }
        let rect = content_rect_screen(s.host, s.titlebar);
        for h in &s.hidden {
            if !last.hidden.contains(h) {
                let hidden = HWND(*h as *mut _);
                if !hidden.0.is_null() {
                    let _ = ShowWindow(hidden, SW_HIDE);
                }
            }
        }
        for h in &last.hidden {
            if !s.hidden.contains(h) && Some(*h) != s.active {
                let shown = HWND(*h as *mut _);
                if !shown.0.is_null() {
                    let _ = ShowWindow(shown, SW_SHOWNA);
                }
            }
        }
        if let Some(a) = s.active {
            let active = HWND(a as *mut _);
            if !active.0.is_null() {
                let switched = last.active != s.active;
                let re_shown = last.hidden.contains(&a);
                let need_show = switched || re_shown;
                if switched || re_shown || last.settle > 0 {
                    if let Some(measured) = member_insets(active) {
                        last.inset = measured;
                    }
                }
                let inset = last.inset;
                if let Some(rect) = rect {
                    let (ox, oy, fw, fh) = inset;
                    let desired = (rect.0 - ox, rect.1 - oy, rect.2 + fw, rect.3 + fh);
                    if last.rect != Some(desired) || switched || re_shown {
                        last.rect = Some(desired);
                        last.settle = SETTLE_PASSES;
                    }
                    if last.settle > 0 {
                        last.settle -= 1;
                        place_member(active, rect, inset, true, false);
                    }
                }
                // Show only after placing, so a freshly added or switched-to
                // member never flashes at its old desktop spot.
                if need_show {
                    let _ = ShowWindow(active, SW_SHOWNA);
                }
                if need_show && GetForegroundWindow() == host {
                    let _ = SetWindowPos(
                        active,
                        Some(host),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
                    );
                }
            }
        }
        last.active = s.active;
        std::mem::swap(&mut last.hidden, &mut s.hidden);
    }
}
