use windows::Win32::Foundation::{GetLastError, SetLastError, HWND, RECT, WIN32_ERROR};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMNCRP_USEWINDOWSTYLE, DWMWA_NCRENDERING_POLICY,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_DONOTROUND,
};
use windows::Win32::Graphics::Gdi::SetWindowRgn;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, IsIconic, IsZoomed, SetWindowLongPtrW,
    SetWindowPlacement, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_HWNDPARENT, GWL_STYLE,
    SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SW_HIDE, SW_RESTORE, SW_SHOWMINIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNA, WINDOWPLACEMENT,
    WINDOW_LONG_PTR_INDEX, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX,
    WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
};

const FRAME: isize = WS_CAPTION.0 as isize
    | WS_THICKFRAME.0 as isize
    | WS_SYSMENU.0 as isize
    | WS_MINIMIZEBOX.0 as isize
    | WS_MAXIMIZEBOX.0 as isize;

#[derive(Clone, Debug)]
pub struct StylingState {
    pub style: isize,
    pub exstyle: isize,
    pub rect: (i32, i32, i32, i32),
    pub placement: WINDOWPLACEMENT,
}

pub(super) fn set_long_ptr(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> bool {
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let prev = SetWindowLongPtrW(hwnd, index, value);
        prev != 0 || GetLastError() == WIN32_ERROR(0)
    }
}

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
        if GetWindowPlacement(mh, &mut placement).is_err() {
            return None;
        }
        let mut r = RECT::default();
        if GetWindowRect(mh, &mut r).is_err() {
            return None;
        }
        let style = GetWindowLongPtrW(mh, GWL_STYLE);
        let exstyle = GetWindowLongPtrW(mh, GWL_EXSTYLE);
        if !set_long_ptr(mh, GWL_HWNDPARENT, host) {
            return None;
        }
        let hosted_ex = exstyle & !(WS_EX_APPWINDOW.0 as isize);
        let _ = set_long_ptr(mh, GWL_EXSTYLE, hosted_ex);
        let _ = set_long_ptr(mh, GWL_STYLE, style & !FRAME);
        set_frame_treatment(mh, true);
        if IsIconic(mh).as_bool() || IsZoomed(mh).as_bool() {
            let _ = ShowWindow(mh, SW_RESTORE);
        }
        // Stay hidden until the worker pins it, so it never flashes at its old
        // desktop position during the hand-off.
        let _ = ShowWindow(mh, SW_HIDE);
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

pub fn unregister_member(member: isize, st: &StylingState) {
    if member == 0 {
        return;
    }
    let mh = HWND(member as *mut _);
    unsafe {
        let minimized = IsIconic(mh).as_bool() || st.placement.showCmd as i32 == SW_SHOWMINIMIZED.0;
        let _ = ShowWindow(mh, SW_HIDE);
        let _ = SetWindowRgn(mh, None, true);
        let _ = set_long_ptr(mh, GWL_HWNDPARENT, 0);
        let _ = set_long_ptr(mh, GWL_STYLE, st.style);
        // Clearing the owner alone leaves the window missing from the taskbar
        // until the user reactivates it; bouncing WS_EX_TOOLWINDOW (framechanged)
        // makes the shell rebuild the button now.
        let ex = GetWindowLongPtrW(mh, GWL_EXSTYLE);
        let _ = set_long_ptr(mh, GWL_EXSTYLE, ex | WS_EX_TOOLWINDOW.0 as isize);
        let _ = SetWindowPos(
            mh,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE
                | SWP_NOSIZE
                | SWP_NOZORDER
                | SWP_FRAMECHANGED
                | SWP_NOACTIVATE
                | SWP_ASYNCWINDOWPOS,
        );
        let _ = set_long_ptr(mh, GWL_EXSTYLE, st.exstyle);
        set_frame_treatment(mh, false);
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
