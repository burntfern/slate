use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC,
    SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassLongPtrW, GetIconInfo, SendMessageTimeoutW, GCLP_HICON, GCLP_HICONSM, HICON, ICONINFO,
    SEND_MESSAGE_TIMEOUT_FLAGS, WM_GETICON,
};

use super::{ICON_BIG, ICON_SMALL, ICON_SMALL2, SMTO_ABORTIFHUNG};

pub struct IconPixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

pub fn window_icon(hwnd: isize) -> Option<IconPixels> {
    if hwnd == 0 {
        return None;
    }
    unsafe {
        let h = HWND(hwnd as *mut _);
        let hicon = query_icon(h)?;
        extract(hicon)
    }
}

unsafe fn query_icon(h: HWND) -> Option<HICON> {
    for which in [ICON_BIG, ICON_SMALL2, ICON_SMALL] {
        let mut result: usize = 0;
        let _ = SendMessageTimeoutW(
            h,
            WM_GETICON,
            WPARAM(which),
            LPARAM(0),
            SEND_MESSAGE_TIMEOUT_FLAGS(SMTO_ABORTIFHUNG),
            200,
            Some(&mut result),
        );
        if result != 0 {
            return Some(HICON(result as *mut _));
        }
    }
    let big = GetClassLongPtrW(h, GCLP_HICON);
    if big != 0 {
        return Some(HICON(big as *mut _));
    }
    let small = GetClassLongPtrW(h, GCLP_HICONSM);
    if small != 0 {
        return Some(HICON(small as *mut _));
    }
    None
}

unsafe fn extract(hicon: HICON) -> Option<IconPixels> {
    let mut info = ICONINFO::default();
    if GetIconInfo(hicon, &mut info).is_err() {
        return None;
    }
    let pixels = decode(info.hbmColor, info.hbmMask);
    if !info.hbmColor.is_invalid() {
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
    }
    if !info.hbmMask.is_invalid() {
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    }
    pixels
}

unsafe fn decode(color: HBITMAP, mask: HBITMAP) -> Option<IconPixels> {
    let mut bm = BITMAP::default();
    if GetObjectW(
        HGDIOBJ(color.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut c_void),
    ) == 0
    {
        return None;
    }
    let width = bm.bmWidth.max(0) as u32;
    let height = bm.bmHeight.max(0) as u32;
    if width == 0 || height == 0 {
        return None;
    }

    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let Some(mut bgra) = dibits(mem, color, width, height) else {
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(None, screen);
        return None;
    };
    let mut alpha_present = false;
    for p in bgra.chunks(4) {
        if p[3] != 0 {
            alpha_present = true;
            break;
        }
    }
    if !alpha_present {
        if let Some(mask_buf) = dibits(mem, mask, width, height) {
            for (px, m) in bgra.chunks_mut(4).zip(mask_buf.chunks(4)) {
                px[3] = 255u8.wrapping_sub(m[0].max(m[1]).max(m[2]));
            }
        } else {
            for px in bgra.chunks_mut(4) {
                px[3] = 255;
            }
        }
    }
    let _ = DeleteDC(mem);
    let _ = ReleaseDC(None, screen);
    Some(IconPixels {
        width,
        height,
        bgra,
    })
}

unsafe fn dibits(mem: HDC, hbm: HBITMAP, width: u32, height: u32) -> Option<Vec<u8>> {
    if hbm.is_invalid() {
        return None;
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
    let old = SelectObject(mem, HGDIOBJ(hbm.0));
    let got = GetDIBits(
        mem,
        hbm,
        0,
        height,
        Some(buf.as_mut_ptr() as *mut c_void),
        &mut info,
        DIB_RGB_COLORS,
    );
    SelectObject(mem, old);
    (got != 0).then_some(buf)
}
