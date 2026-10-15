//! Window hosting (United Sets mechanism): own, reposition, crop, show/hide, style save/restore, detach.
//!
//! Hosts real foreign windows inside a Slate host window by taking ownership (GWL_HWNDPARENT) and
//! repositioning them to the host's content rect. No SetParent/WS_CHILD reparenting.
//! Only the active member is visible at a time. On user drag-out, the tab is ungrouped and style restored.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use super::stack::scale_factor;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMNCRP_USEWINDOWSTYLE, DWMWA_NCRENDERING_POLICY,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetClientRect, GetForegroundWindow, GetWindowLongPtrW, GetWindowPlacement,
    GetWindowRect, IsIconic, IsWindowVisible, IsZoomed, SetWindowLongPtrW, SetWindowPlacement,
    SetWindowPos, ShowWindow, GWLP_WNDPROC, GWL_EXSTYLE, GWL_HWNDPARENT, GWL_STYLE,
    SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSENDCHANGING,
    SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOWMINIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNA,
    WINDOWPLACEMENT, WM_MOVE, WM_SIZE, WM_WINDOWPOSCHANGED, WNDPROC, WS_CAPTION, WS_EX_APPWINDOW,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
};

/// The window frame bits stripped when a member is hosted borderless.
const FRAME: isize = WS_CAPTION.0 as isize
    | WS_THICKFRAME.0 as isize
    | WS_SYSMENU.0 as isize
    | WS_MINIMIZEBOX.0 as isize
    | WS_MAXIMIZEBOX.0 as isize;

/// Saved window styling/placement before hosting. Restoring this puts the window
/// back exactly as Slate found it (styles, owner, rect, minimized/maximized state).
#[derive(Clone, Debug)]
pub struct StylingState {
    pub style: isize,
    pub exstyle: isize,
    pub rect: (i32, i32, i32, i32),
    pub placement: WINDOWPLACEMENT,
}

#[derive(Clone, Debug)]
struct HostShared {
    host: isize,
    titlebar: f32, // logical pixels
    active: Option<isize>,
    hidden: Vec<isize>,
    /// The active member's frame insets `(offset_x, offset_y, frame_w, frame_h)`
    /// as last measured by the worker, so `retrack` can client-align during a
    /// drag without cross-process calls on the UI thread.
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

/// Reposition worker for the active host group. It applies shared state and
/// pushes owned `SetWindowPos`/`ShowWindow` calls; it never touches GPUI or
/// `core` (spec 01). It sleeps until `update` signals a change (or a 50 ms
/// safety timeout elapses), so it is idle when nothing moves; per-frame drag
/// tracking is handled synchronously by `host_proc`/`retrack`.
#[derive(Clone)]
pub struct HostWorker {
    shared: Arc<Mutex<HostShared>>,
    running: Arc<AtomicBool>,
    /// Signalled by `update`/`stop` to wake the worker immediately.
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
            wake: Arc::new((Mutex::new(()), Condvar::new())),
        }
    }

    /// Publish the current host layout.
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
        let shared = self.shared.clone();
        let running = self.running.clone();
        let wake = self.wake.clone();
        thread::spawn(move || {
            let mut last = WorkerState::default();
            while running.load(Ordering::Relaxed) {
                // Block until `update` signals a change, or 50 ms elapses as a
                // safety net for changes we did not initiate. Drags are re-pinned
                // synchronously by `retrack`, so the timeout only covers show/hide
                // and one-shot placements. The worker is idle when nothing moves.
                {
                    let (lock, cvar) = &*wake;
                    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = cvar
                        .wait_timeout(guard, std::time::Duration::from_millis(50))
                        .unwrap_or_else(|e| e.into_inner());
                }
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                // Snapshot under the lock, then release it. The lock is NEVER
                // held across Win32 calls: SetWindowPos/ShowWindow reach other
                // processes' threads and can block, and holding the mutex there
                // would deadlock the UI thread's `sync_worker`.
                let snap = {
                    let s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    Snapshot {
                        host: s.host,
                        titlebar: s.titlebar,
                        active: s.active,
                        hidden: s.hidden.clone(),
                    }
                };
                apply(&snap, &mut last);
                // Publish the measured insets so `retrack` can client-align
                // during a drag without cross-process calls on the UI thread.
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

    /// Subclass the host window so the active member is repositioned the moment
    /// the host moves/resizes, in the host's own thread. The worker sleeps until
    /// signalled, so during a drag the member would lag until the next wake.
    /// Handling `WM_WINDOWPOSCHANGED` on the host thread re-pins the member
    /// synchronously (`retrack`) so it moves in lockstep.
    pub fn install_move_tracker(&self, host: isize) {
        if host == 0 || TRACKER_ORIG.with(|o| o.get()) != 0 {
            return;
        }
        let shared = self.shared.clone();
        TRACKER.with(|t| *t.borrow_mut() = Some(shared));
        unsafe {
            let hwnd = HWND(host as *mut _);
            let orig = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
            if orig == 0 {
                return;
            }
            TRACKER_ORIG.with(|o| o.set(orig));
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, host_proc as *const () as isize);
        }
    }
}

thread_local! {
    static TRACKER: RefCell<Option<Arc<Mutex<HostShared>>>> = const { RefCell::new(None) };
    static TRACKER_ORIG: Cell<isize> = const { Cell::new(0) };
}

/// Host window procedure shim: chain to gpui's proc, then re-pin the member.
unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let orig = TRACKER_ORIG.with(|o| o.get());
    let result = CallWindowProcW(std::mem::transmute::<isize, WNDPROC>(orig), hwnd, msg, w, l);
    if msg == WM_WINDOWPOSCHANGED || msg == WM_MOVE || msg == WM_SIZE {
        // Re-crop only when the host size (hence member size) changed; a pure
        // move leaves the window-coordinate crop region valid.
        retrack(msg == WM_SIZE);
    }
    result
}

/// Immediately pin the active member to the host content rect (non-blocking).
///
/// Runs on the host's own thread for every move/resize, so it re-measures the
/// member's frame insets rather than trusting `shared.inset`: apps that
/// re-assert an invisible DWM frame (Explorer, Notepad, WinUI) do so whenever
/// they like, and the published value can be up to a worker period stale — a
/// mid-drag re-assert would otherwise leave the client off by the border width
/// until the next worker pass caught up. Falls back to the published insets if
/// the measurement itself fails.
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
    // Synchronous (no SWP_ASYNCWINDOWPOS): during a drag the member is idle, so
    // moving it in lockstep is instant; the async post added a frame of lag,
    // which is what made the member trail the host while dragging.
    place_member(active, content, inset, resize, false);
}

/// Measured frame insets of `mh`: `(offset_x, offset_y, frame_w, frame_h)` where
/// `offset_*` is the client origin within the window and `frame_*` the total
/// non-client width/height. `None` when any of the three queries fails (the
/// window is being destroyed, or its thread is hung) — callers then fall back to
/// the last known-good insets instead of treating the window as frameless.
fn member_insets(mh: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut wr = RECT::default();
        let mut cr = RECT::default();
        let mut origin = POINT { x: 0, y: 0 };
        if GetWindowRect(mh, &mut wr).is_err()
            || GetClientRect(mh, &mut cr).is_err()
            || !ClientToScreen(mh, &mut origin).as_bool()
        {
            return None;
        }
        let ox = origin.x - wr.left;
        let oy = origin.y - wr.top;
        let fw = (wr.right - wr.left) - (cr.right - cr.left);
        let fh = (wr.bottom - wr.top) - (cr.bottom - cr.top);
        Some((ox, oy, fw.max(0), fh.max(0)))
    }
}

/// Position the active member so its *client* area maps exactly onto `content`,
/// then (when `crop`) clip the window to that client area.
///
/// Apps that re-assert an invisible DWM frame (File Explorer, Notepad, other
/// WinUI) keep a non-zero non-client border. Placing the *window* rect onto the
/// content rect then insets the visible client on every side (the "not full
/// width" gaps). Expanding the window by the frame insets makes the client fill
/// the content, and `SetWindowRgn` crops the overhang away so nothing spills
/// past the host. Re-measured every pass, so a late frame re-assert self-heals.
fn place_member(
    mh: HWND,
    content: (i32, i32, i32, i32),
    inset: (i32, i32, i32, i32),
    crop: bool,
    posted: bool,
) {
    let (cl, ct, cw, ch) = content;
    let (ox, oy, fw, fh) = inset;
    let x = cl - ox;
    let y = ct - oy;
    let w = (cw + fw).max(1);
    let h = (ch + fh).max(1);
    let mut flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSENDCHANGING;
    if posted {
        flags |= SWP_ASYNCWINDOWPOS;
    }
    unsafe {
        let _ = SetWindowPos(mh, None, x, y, w, h, flags);
        // Only crop when there is an actual frame to hide; a frameless window's
        // region would be the whole window anyway.
        if crop && (ox != 0 || oy != 0 || fw != 0 || fh != 0) {
            let rgn = CreateRectRgn(ox, oy, ox + cw, oy + ch);
            if !rgn.0.is_null() && SetWindowRgn(mh, Some(rgn), true) == 0 {
                // On failure the system does not take ownership; release it.
                let _ = DeleteObject(HGDIOBJ(rgn.0));
            }
        }
    }
}

impl Drop for HostWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A lock-free copy of the published layout.
#[derive(Clone, Default)]
struct Snapshot {
    host: isize,
    titlebar: f32,
    active: Option<isize>,
    hidden: Vec<isize>,
}

/// What the worker last applied, so redundant cross-process calls are skipped.
#[derive(Default)]
struct WorkerState {
    active: Option<isize>,
    hidden: Vec<isize>,
    /// The desired *window* rect (content client-aligned by the frame insets).
    rect: Option<(i32, i32, i32, i32)>,
    /// Last measured member frame insets, republished for `retrack`.
    inset: (i32, i32, i32, i32),
}

/// One reposition pass: pin the active member to the content rect, keep the
/// rest hidden, and raise the active member above the host when it changes.
fn apply(s: &Snapshot, last: &mut WorkerState) {
    let host = HWND(s.host as *mut _);
    if host.0.is_null() {
        return;
    }
    unsafe {
        // A minimized or hidden host takes its owned members down with it;
        // don't re-show them, or they would float alone on the desktop.
        if IsIconic(host).as_bool() || !IsWindowVisible(host).as_bool() {
            return;
        }
        let rect = content_rect_screen(s.host, s.titlebar);
        // Hide windows that just became inactive; leave the rest untouched.
        for h in &s.hidden {
            if !last.hidden.contains(h) {
                let hidden = HWND(*h as *mut _);
                if !hidden.0.is_null() {
                    let _ = ShowWindow(hidden, SW_HIDE);
                }
            }
        }
        // Re-show windows that left the hidden set (e.g. the overlay/menu closed).
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
                // Was hidden last pass (inactive tab, or the menu overlay was up):
                // it must be shown again even if the active member did not change.
                let re_shown = last.hidden.contains(&a);
                if switched || re_shown {
                    let _ = ShowWindow(active, SW_SHOWNA);
                }
                // Measure the member's live frame insets every pass. Apps that
                // re-assert an invisible DWM frame (Explorer, Notepad, WinUI) do
                // so asynchronously after we strip it, so a one-shot read at
                // register time would miss it and leave the client inset. A
                // failed read keeps the last known-good insets rather than
                // collapsing the window onto a borderless origin.
                if let Some(measured) = member_insets(active) {
                    last.inset = measured;
                }
                let inset = last.inset;
                if let Some(rect) = rect {
                    // Client-align: expand the window by the frame insets so its
                    // client area maps 1:1 onto the content rect.
                    let (ox, oy, fw, fh) = inset;
                    let desired = (rect.0 - ox, rect.1 - oy, rect.2 + fw, rect.3 + fh);
                    if last.rect != Some(desired) || switched || re_shown {
                        place_member(active, rect, inset, true, true);
                        last.rect = Some(desired);
                    }
                }
                // Keep the member above the host when the host is focused.
                if (switched || re_shown) && GetForegroundWindow() == host {
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
        last.hidden = s.hidden.clone();
    }
}

/// Save styling and set the host as owner (GWL_HWNDPARENT). Ownership only —
/// never `WS_CHILD`. The native frame is stripped so the content maps 1:1 onto
/// the host rect (square, edge-to-edge); the original style is always saved for
/// restore.
pub fn register_member(host: isize, member: isize) -> Option<StylingState> {
    if host == 0 || member == 0 {
        return None;
    }
    let mh = HWND(member as *mut _);
    unsafe {
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        let _ = GetWindowPlacement(mh, &mut placement);
        let mut r = RECT::default();
        let _ = GetWindowRect(mh, &mut r);
        let style = GetWindowLongPtrW(mh, GWL_STYLE);
        let exstyle = GetWindowLongPtrW(mh, GWL_EXSTYLE);
        // Own the window (ownership only, never WS_CHILD).
        SetWindowLongPtrW(mh, GWL_HWNDPARENT, host);
        // Drop it from the taskbar and Alt+Tab while hosted: ownership hides an
        // owned window unless it carries WS_EX_APPWINDOW. We only clear
        // WS_EX_APPWINDOW (never add WS_EX_TOOLWINDOW) so the effect is
        // self-healing: if Slate dies, ownership is severed and the window
        // reappears in the taskbar on its own. Restored from `exstyle` on unregister.
        let hosted_ex = exstyle & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(mh, GWL_EXSTYLE, hosted_ex);
        // Borderless layout: strip the frame so the window has no title bar of
        // its own and the content rectangle maps 1:1 onto the host's designated
        // area (square, no frame padding). Restored from `style` on unregister.
        SetWindowLongPtrW(mh, GWL_STYLE, style & !FRAME);
        // Slate chooses this automatically per hosted window; it is reverted on
        // unregister, so it never affects the window once it leaves Slate.
        set_frame_treatment(mh, true);
        // Restore if minimized or maximized so it can be repositioned into the
        // host content rect. Otherwise a maximized window stays maximized (it
        // ignores the new size and covers the whole screen above the host),
        // which makes the host appear to vanish. `placement` restores the
        // maximized/minimized state on unregister.
        if IsIconic(mh).as_bool() || IsZoomed(mh).as_bool() {
            let _ = ShowWindow(mh, SW_RESTORE);
        }
        let _ = SetWindowPos(
            mh,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_ASYNCWINDOWPOS,
        );
        Some(StylingState {
            style,
            exstyle,
            rect: (r.left, r.top, r.right, r.bottom),
            placement,
        })
    }
}

/// Switch a member's DWM frame treatment on host / off restore. Apps that
/// re-assert their own frame (Chrome, Terminal, WinUI) keep an invisible DWM
/// border even after the style bits are stripped, which insets the content so
/// the window no longer fills the host rect. Disabling non-client rendering on
/// the hosted window drops that border, and `DWMWCP_DONOTROUND` drops the Win11
/// rounding, so the client area maps 1:1 onto the host content rect.
fn set_frame_treatment(mh: HWND, hosted: bool) {
    let (corner, ncrp) = if hosted {
        (DWMWCP_DONOTROUND, DWMNCRP_DISABLED)
    } else {
        (DWMWCP_DEFAULT, DWMNCRP_USEWINDOWSTYLE)
    };
    unsafe {
        let _ = DwmSetWindowAttribute(
            mh,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<windows::Win32::Graphics::Dwm::DWM_WINDOW_CORNER_PREFERENCE>()
                as u32,
        );
        let _ = DwmSetWindowAttribute(
            mh,
            DWMWA_NCRENDERING_POLICY,
            &ncrp as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<windows::Win32::Graphics::Dwm::DWMNCRENDERINGPOLICY>() as u32,
        );
    }
}

/// Restore styling/placement and clear ownership. Leaves the window exactly as
/// Slate found it: never hidden, never closed, never terminated.
pub fn unregister_member(member: isize, st: &StylingState) {
    if member == 0 {
        return;
    }
    let mh = HWND(member as *mut _);
    unsafe {
        // Whether the shell should show it minimized once restored.
        let minimized = IsIconic(mh).as_bool() || st.placement.showCmd as i32 == SW_SHOWMINIMIZED.0;
        // Force the shell to re-evaluate the taskbar button: re-adding
        // WS_EX_APPWINDOW and clearing the owner at runtime does NOT register the
        // window in the taskbar on its own — it stays missing until the user
        // reactivates it by hand. A hide/show cycle makes it reappear.
        let _ = ShowWindow(mh, SW_HIDE);
        // Drop any hosting crop so the restored window is not clipped.
        let _ = SetWindowRgn(mh, None, true);
        SetWindowLongPtrW(mh, GWL_HWNDPARENT, 0);
        SetWindowLongPtrW(mh, GWL_STYLE, st.style);
        SetWindowLongPtrW(mh, GWL_EXSTYLE, st.exstyle);
        // Leave the window looking exactly as Slate found it.
        set_frame_treatment(mh, false);
        // Placement restores both the rect and the minimized/maximized state.
        let _ = SetWindowPlacement(mh, &st.placement);
        let (l, t, r, b) = st.rect;
        let w = (r - l).max(0);
        let h = (b - t).max(0);
        let _ = SetWindowPos(
            mh,
            None,
            l,
            t,
            w,
            h,
            SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        );
        // Show it back (never activating; the controller re-focuses the active
        // member) without un-minimizing a window that was minimized.
        let _ = ShowWindow(
            mh,
            if minimized {
                SW_SHOWMINNOACTIVE
            } else {
                SW_SHOWNA
            },
        );
    }
}

/// Host content rect in screen coordinates (below the title bar), as
/// `(left, top, width, height)`. The title bar height is logical pixels on the
/// host's monitor; it is scaled by the window's DPI (PerMonitorV2, spec 02/06).
pub fn content_rect_screen(host: isize, titlebar_logical: f32) -> Option<(i32, i32, i32, i32)> {
    if host == 0 {
        return None;
    }
    let hh = HWND(host as *mut _);
    unsafe {
        let mut cr = RECT::default();
        if GetClientRect(hh, &mut cr).is_err() {
            return None;
        }
        let mut origin = POINT { x: 0, y: 0 };
        if !ClientToScreen(hh, &mut origin).as_bool() {
            return None;
        }
        let title_px = ((titlebar_logical * scale_factor(host)) as i32).max(0);
        let width = (cr.right - cr.left).max(1);
        let height = ((cr.bottom - cr.top) - title_px).max(1);
        Some((origin.x, origin.y + title_px, width, height))
    }
}
