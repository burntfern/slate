//! Window icon extraction (HICON -> RGBA pixels).
//!
//! The tab bar can show a host's real icon. That means pulling the `HICON`
//! Windows associates with the window (`WM_GETICON`, then the class icon) and
//! turning it into a top-down RGBA buffer the UI can turn into an image.

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetObjectW, ReleaseDC, SelectObject, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetClassLongPtrW, GetIconInfo, SendMessageTimeoutW, SendMessageW,
    GCLP_HICON, GCLP_HICONSM, HICON, ICONINFO, SEND_MESSAGE_TIMEOUT_FLAGS, WM_GETICON, WM_SETICON,
};

/// A window icon decoded to top-down, 8-bit-per-channel RGBA.
pub struct IconPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

// WM_GETICON wParam values and SMTO_ABORTIFHUNG.
const ICON_SMALL: usize = 0;
const ICON_BIG: usize = 1;
const ICON_SMALL2: usize = 2;
const SMTO_ABORTIFHUNG: u32 = 0x0002;

/// Install the selected logo (`SLATE_LOGO`, chosen by the `SLATE_ICON` build
/// var) as the window's taskbar / Alt-Tab icon (gpui-pre has no Windows
/// window-icon API). Builds HICONs from the PNG and hands them to the shell via
/// `WM_SETICON`. Never panics.
///
/// The big icon is built at 256px so Windows has real pixels to downscale from
/// at high DPI (a 32px icon upscaled to a 48px taskbar slot looked blurry).
pub fn install_window_icon(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    const PNG: &[u8] = include_bytes!(env!("SLATE_LOGO"));
    let Ok(image) = image::load_from_memory(PNG) else {
        return;
    };
    let rgba = image.to_rgba8();
    unsafe {
        let h = HWND(hwnd as *mut _);
        for (which, size) in [(ICON_BIG, 256u32), (ICON_SMALL, 32u32)] {
            if let Some(icon) = hicon_from_rgba(&rgba, size) {
                SendMessageW(h, WM_SETICON, Some(WPARAM(which)), Some(LPARAM(icon)));
            }
        }
    }
}

/// Build an alpha-preserving HICON of `size`x`size` from an RGBA image.
unsafe fn hicon_from_rgba(image: &image::RgbaImage, size: u32) -> Option<isize> {
    let resized = image::imageops::resize(image, size, size, image::imageops::FilterType::Lanczos3);
    let (w, h) = (size as i32, size as i32);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            ..Default::default()
        },
        ..Default::default()
    };
    let screen = GetDC(None);
    let mut bits: *mut c_void = std::ptr::null_mut();
    let color = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0);
    let _ = ReleaseDC(None, screen);
    let color = match color {
        Ok(hbm) if !hbm.is_invalid() && !bits.is_null() => hbm,
        _ => return None,
    };
    // Straight alpha: copy RGBA -> BGRA.
    let dst =
        std::slice::from_raw_parts_mut(bits as *mut u8, (size as usize) * (size as usize) * 4);
    for (i, px) in resized.pixels().enumerate() {
        let [r, g, b, a] = px.0;
        dst[i * 4] = b;
        dst[i * 4 + 1] = g;
        dst[i * 4 + 2] = r;
        dst[i * 4 + 3] = a;
    }
    let mask = CreateBitmap(w, h, 1, 1, None);
    let icon = ICONINFO {
        fIcon: TRUE,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let result = CreateIconIndirect(&icon)
        .ok()
        .filter(|hicon| !hicon.is_invalid());
    if !mask.is_invalid() {
        let _ = DeleteObject(HGDIOBJ(mask.0));
    }
    let _ = DeleteObject(HGDIOBJ(color.0));
    result.map(|hicon| hicon.0 as isize)
}

/// Decode the icon of `hwnd`, if it has one. Never panics; returns `None` on any
/// API failure or an empty bitmap.
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

unsafe fn decode(
    color: windows::Win32::Graphics::Gdi::HBITMAP,
    mask: windows::Win32::Graphics::Gdi::HBITMAP,
) -> Option<IconPixels> {
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
    let mut rgba = dibits(mem, color, width, height)?;
    let mut alpha_present = false;
    for p in rgba.chunks(4) {
        if p[3] != 0 {
            alpha_present = true;
            break;
        }
    }
    if !alpha_present {
        // Legacy icons carry transparency in a 1bpp AND-mask, not in alpha.
        if let Some(mask_buf) = dibits(mem, mask, width, height) {
            for (px, m) in rgba.chunks_mut(4).zip(mask_buf.chunks(4)) {
                // Mask white (255) means "transparent".
                px[3] = 255u8.wrapping_sub(m[0].max(m[1]).max(m[2]));
            }
        } else {
            for px in rgba.chunks_mut(4) {
                px[3] = 255;
            }
        }
    }
    let _ = DeleteDC(mem);
    let _ = ReleaseDC(None, screen);
    for px in rgba.chunks_mut(4) {
        px.swap(0, 2); // BGRA -> RGBA
    }
    Some(IconPixels {
        width,
        height,
        rgba,
    })
}

unsafe fn dibits(
    mem: windows::Win32::Graphics::Gdi::HDC,
    hbm: windows::Win32::Graphics::Gdi::HBITMAP,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    if hbm.is_invalid() {
        return None;
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
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
