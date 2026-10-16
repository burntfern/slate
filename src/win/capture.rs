use std::cell::RefCell;

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    GetDIBits, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, BITMAPINFO,
    BITMAPINFOHEADER, DIB_RGB_COLORS, HALFTONE, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

const MAX_W: i32 = 512;

pub struct WindowSnapshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn capture_window(hwnd: isize) -> Option<WindowSnapshot> {
    if hwnd == 0 {
        return None;
    }
    unsafe { grab(hwnd) }
}

struct ScreenDc(HDC);
impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe { ReleaseDC(None, self.0) };
    }
}

struct MemDc(HDC);
impl Drop for MemDc {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.0) };
    }
}

struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { DeleteObject(HGDIOBJ(self.0 .0)) };
        }
    }
}

struct Pooled {
    w: i32,
    h: i32,
    mem: MemDc,
    bmp: Bitmap,
}

thread_local! {
    static POOL: RefCell<Option<Pooled>> = const { RefCell::new(None) };
}

unsafe fn grab(hwnd: isize) -> Option<WindowSnapshot> {
    let h = HWND(hwnd as *mut _);
    let mut rc = RECT::default();
    GetClientRect(h, &mut rc).ok()?;
    let (w, hgt) = (rc.right - rc.left, rc.bottom - rc.top);
    if w <= 0 || hgt <= 0 {
        return None;
    }
    let mut origin = POINT::default();
    if !ClientToScreen(h, &mut origin).as_bool() {
        return None;
    }

    let tw = w.clamp(1, MAX_W);
    let th = ((hgt as f32 * tw as f32 / w as f32).round() as i32).max(1);

    let screen = ScreenDc(GetDC(None));
    let cached = POOL.with(|p| {
        let mut slot = p.borrow_mut();
        match slot.as_ref() {
            Some(existing) if existing.w == tw && existing.h == th => slot.take(),
            _ => {
                *slot = None;
                None
            }
        }
    });
    let pooled = match cached {
        Some(p) => p,
        None => {
            let mem = MemDc(CreateCompatibleDC(Some(screen.0)));
            let bmp = Bitmap(CreateCompatibleBitmap(screen.0, tw, th));
            if bmp.0.is_invalid() {
                return None;
            }
            Pooled {
                w: tw,
                h: th,
                mem,
                bmp,
            }
        }
    };

    let old = SelectObject(pooled.mem.0, HGDIOBJ(pooled.bmp.0 .0));
    SetStretchBltMode(pooled.mem.0, HALFTONE);
    let drawn = StretchBlt(
        pooled.mem.0,
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
    let rgba = drawn
        .then(|| read_pixels(pooled.mem.0, pooled.bmp.0, tw, th))
        .flatten();
    SelectObject(pooled.mem.0, old);
    POOL.with(|p| *p.borrow_mut() = Some(pooled));

    Some(WindowSnapshot {
        width: tw as u32,
        height: th as u32,
        rgba: rgba?,
    })
}

unsafe fn read_pixels(mem: HDC, bmp: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
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
    let (chunks, _) = buf.as_chunks_mut::<4>();
    for px in chunks {
        let v = u32::from_le_bytes(*px);
        let out = 0xFF00_0000 | ((v & 0x0000_00FF) << 16) | (v & 0x0000_FF00) | ((v >> 16) & 0xFF);
        *px = out.to_le_bytes();
    }
    Some(buf)
}
