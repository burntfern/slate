use std::ffi::c_void;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, LoadImageW, SendMessageW, HICON, ICONINFO, IMAGE_FLAGS,
    IMAGE_ICON, WM_SETICON,
};

use super::{ICON_BIG, ICON_SMALL};

pub fn install_window_icon(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let h = HWND(hwnd as *mut _);
        for (which, size) in [(ICON_BIG, 256u32), (ICON_SMALL, 32u32)] {
            let icon = embedded_icon(size).or_else(|| png_hicon(size));
            if let Some(icon) = icon {
                let prev = SendMessageW(h, WM_SETICON, Some(WPARAM(which)), Some(LPARAM(icon)));
                if prev.0 != 0 {
                    let _ = DestroyIcon(HICON(prev.0 as *mut _));
                }
            }
        }
    }
}

#[allow(clippy::manual_dangling_ptr)]
fn embedded_icon(size: u32) -> Option<isize> {
    unsafe {
        let hinst = GetModuleHandleW(None).ok()?;
        let handle = LoadImageW(
            Some(HINSTANCE(hinst.0)),
            PCWSTR::from_raw(1usize as *const u16),
            IMAGE_ICON,
            size as i32,
            size as i32,
            IMAGE_FLAGS(0),
        )
        .ok()?;
        if handle.is_invalid() {
            return None;
        }
        Some(handle.0 as isize)
    }
}

fn png_hicon(size: u32) -> Option<isize> {
    const PNG: &[u8] = include_bytes!(env!("SLATE_LOGO"));
    let image = image::load_from_memory(PNG).ok()?.to_rgba8();
    unsafe { hicon_from_rgba(&image, size) }
}

pub fn app_hicon(size: u32) -> Option<isize> {
    const PNG: &[u8] = include_bytes!(env!("SLATE_LOGO"));
    let image = image::load_from_memory(PNG).ok()?.to_rgba8();
    unsafe { hicon_from_rgba(&image, size) }
}

unsafe fn hicon_from_rgba(image: &image::RgbaImage, size: u32) -> Option<isize> {
    let resized = image::imageops::resize(image, size, size, image::imageops::FilterType::Lanczos3);
    let (w, h) = (size as i32, size as i32);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
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
