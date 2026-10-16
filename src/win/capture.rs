//! Snapshot capture: a hosted window's pixels, for the hover preview.
//!
//! Inactive members are hidden, so there is nothing to sample from them — only
//! the active one is on screen. This grabs the screen pixels under the member's
//! client rect and downscales them, mirroring what gpui's `IconPixels` feeds to
//! `render_image`.

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, DIB_RGB_COLORS, HALFTONE, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

/// Frames are downscaled to at most this many pixels wide: the card shows a
/// thumbnail, so full-resolution captures would only cost memory.
const MAX_W: i32 = 512;

/// A captured frame: top-down, 8-bit RGBA (alpha forced opaque).
pub struct WindowSnapshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Grab the on-screen pixels of `hwnd`'s client area, downscaled to at most
/// `MAX_W` wide. `None` on any failure or an empty rect; never panics.
pub fn capture_window(hwnd: isize) -> Option<WindowSnapshot> {
    if hwnd == 0 {
        return None;
    }
    unsafe { grab(hwnd) }
}

/// The screen DC, released on drop.
struct ScreenDc(HDC);
impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe { ReleaseDC(None, self.0) };
    }
}

/// A memory DC, deleted on drop.
struct MemDc(HDC);
impl Drop for MemDc {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.0) };
    }
}

/// A GDI bitmap, deleted on drop.
struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
        }
    }
}

unsafe fn grab(hwnd: isize) -> Option<WindowSnapshot> {
    let h = HWND(hwnd as *mut _);
    let mut rc = RECT::default();
    GetClientRect(h, &mut rc).ok()?;
    let (w, hgt) = (rc.right - rc.left, rc.bottom - rc.top);
    if w <= 0 || hgt <= 0 {
        return None;
    }
    // The client area's top-left in screen coordinates.
    let mut origin = POINT::default();
    if !ClientToScreen(h, &mut origin).as_bool() {
        return None;
    }

    // Preserve the aspect ratio while capping the width.
    let tw = w.clamp(1, MAX_W);
    let th = ((hgt as f32 * tw as f32 / w as f32).round() as i32).max(1);

    let screen = ScreenDc(GetDC(None));
    let mem = MemDc(CreateCompatibleDC(Some(screen.0)));
    let bmp = Bitmap(CreateCompatibleBitmap(screen.0, tw, th));
    if bmp.0.is_invalid() {
        return None;
    }

    let old = SelectObject(mem.0, HGDIOBJ(bmp.0.0));
    SetStretchBltMode(mem.0, HALFTONE);
    let drawn = StretchBlt(
        mem.0,
        0,
        0,
        tw,
        th,
        Some(screen.0),
        origin.x,
        origin.y,
        w,
        hgt,
        SRCCOPY,
    )
    .as_bool();
    let rgba = drawn.then(|| read_pixels(mem.0, bmp.0, tw, th)).flatten();
    SelectObject(mem.0, old);

    Some(WindowSnapshot {
        width: tw as u32,
        height: th as u32,
        rgba: rgba?,
    })
}

/// Read a 32bpp top-down DIB out of `bmp` and swap BGRA -> RGBA. GDI bitmaps
/// carry no alpha channel, so every pixel is forced opaque.
unsafe fn read_pixels(mem: HDC, bmp: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
    let rows = GetDIBits(
        mem,
        bmp,
        0,
        height as u32,
        Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
        &mut info,
        DIB_RGB_COLORS,
    );
    if rows == 0 {
        return None;
    }
    for px in buf.chunks_mut(4) {
        px.swap(0, 2); // BGRA -> RGBA
        px[3] = 255;
    }
    Some(buf)
}
