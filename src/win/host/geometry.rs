use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, SetWindowPos, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
    SWP_NOSENDCHANGING, SWP_NOZORDER,
};

use super::super::stack::scale_factor;

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
        let title_px = (titlebar_logical * scale_factor(host)).round().max(0.0) as i32;
        let width = (cr.right - cr.left).max(1);
        let height = ((cr.bottom - cr.top) - title_px).max(1);
        Some((origin.x, origin.y + title_px, width, height))
    }
}

pub(super) fn member_insets(mh: HWND) -> Option<(i32, i32, i32, i32)> {
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

pub(super) fn place_member(
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
        if crop && (ox != 0 || oy != 0 || fw != 0 || fh != 0) {
            let rgn = CreateRectRgn(ox, oy, ox + cw, oy + ch);
            if !rgn.0.is_null() && SetWindowRgn(mh, Some(rgn), true) == 0 {
                let _ = DeleteObject(HGDIOBJ(rgn.0));
            }
        }
    }
}
