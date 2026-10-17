use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;

use windows::core::{w, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONINFORMATION, MB_OK, SW_SHOWNORMAL,
};

// One long-lived worker opens URLs, so repeated clicks cannot spawn threads.
fn sender() -> &'static Sender<String> {
    static SENDER: OnceLock<Sender<String>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for url in rx {
                let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
                unsafe {
                    // A return value <= 32 means failure; nothing actionable, so ignored.
                    let _ = ShellExecuteW(
                        None,
                        w!("open"),
                        PCWSTR(wide.as_ptr()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    );
                }
            }
        });
        tx
    })
}

pub fn open_url(url: &str) {
    let _ = sender().send(url.to_owned());
}

/// Blocking, but only used when a second copy quits immediately at startup.
pub fn info_message(title: &str, body: &str) {
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let title = wide(title);
    let body = wide(body);
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(title.as_ptr()),
            PCWSTR(body.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
