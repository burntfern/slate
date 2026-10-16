use crate::core::WindowIdentity;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::PROCESS_NAME_WIN32;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

thread_local! {
    static FOUND: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
    static TEXT_BUF: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static CLASS_BUF: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

struct PidMeta {
    exe_path: String,
    elevated: bool,
}

pub fn enumerate_windows() -> Vec<WindowIdentity> {
    let our_pid = std::process::id();
    FOUND.with(|f| f.borrow_mut().clear());
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(0));
    }
    let mut cache: HashMap<u32, PidMeta> = HashMap::new();
    FOUND.with(|f| {
        f.borrow()
            .iter()
            .filter_map(|&hwnd| unsafe { metadata(hwnd, our_pid, &mut cache) })
            .collect()
    })
}

unsafe extern "system" fn enum_proc(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    if is_candidate(hwnd) {
        FOUND.with(|f| f.borrow_mut().push(hwnd));
    }
    BOOL(1)
}

unsafe fn is_candidate(hwnd: HWND) -> bool {
    if !IsWindowVisible(hwnd).as_bool() {
        return false;
    }
    if GetWindow(hwnd, GW_OWNER).is_ok() {
        return false;
    }
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TOOLWINDOW.0 != 0 || ex & WS_EX_NOACTIVATE.0 != 0 {
        return false;
    }
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return false;
    }
    if rect.right - rect.left <= 0 || rect.bottom - rect.top <= 0 {
        return false;
    }
    let mut cloaked: u32 = 0;
    let size = std::mem::size_of::<u32>() as u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, size).is_ok()
        && cloaked != 0
    {
        return false;
    }
    true
}

pub fn window_title(hwnd: isize) -> String {
    unsafe { window_text(HWND(hwnd as *mut _)) }
}

unsafe fn metadata(
    hwnd: HWND,
    our_pid: u32,
    cache: &mut HashMap<u32, PidMeta>,
) -> Option<WindowIdentity> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == our_pid {
        return None;
    }
    let title = window_text(hwnd);
    let class = window_class(hwnd);
    let meta = cache.entry(pid).or_insert_with(|| PidMeta {
        exe_path: process_path(pid).unwrap_or_default(),
        elevated: super::integrity::is_elevated(pid),
    });
    let exe_name = Path::new(&meta.exe_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| class.clone());
    Some(WindowIdentity {
        hwnd: hwnd.0 as isize,
        pid,
        exe_path: meta.exe_path.clone(),
        exe_name,
        title,
        class,
        elevated: meta.elevated,
    })
}

unsafe fn window_text(hwnd: HWND) -> String {
    TEXT_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        buf.clear();
        buf.resize(512, 0);
        let len = GetWindowTextW(hwnd, buf.as_mut_slice());
        if len <= 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..len as usize])
                .trim()
                .to_string()
        }
    })
}

unsafe fn window_class(hwnd: HWND) -> String {
    CLASS_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        buf.clear();
        buf.resize(256, 0);
        let len = GetClassNameW(hwnd, buf.as_mut_slice());
        if len <= 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..len as usize])
                .trim()
                .to_string()
        }
    })
}

fn wide_to_string(buf: &[u16]) -> String {
    String::from_utf16_lossy(buf).trim().to_string()
}

fn process_path(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        buf.truncate(len as usize);
        Some(wide_to_string(&buf))
    }
}
