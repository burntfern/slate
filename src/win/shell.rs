use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::{w, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONINFORMATION, MB_OK, SW_SHOWNORMAL,
};

fn shell_execute(verb: PCWSTR, target: &[u16]) {
    unsafe {
        let _ = ShellExecuteW(
            None,
            verb,
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn open_url(url: &str) {
    let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    std::thread::spawn(move || shell_execute(w!("open"), &target));
}

pub fn open_folder(path: &Path) {
    let target: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    std::thread::spawn(move || shell_execute(w!("explore"), &target));
}

pub fn info_message(title: &str, body: &str) {
    let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let body: Vec<u16> = body.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(title.as_ptr()),
            PCWSTR(body.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
