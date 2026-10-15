//! Compatibility helpers for window hosting (blacklist + modes).
//!
//! Hosts must never touch certain system windows (taskbar/desktop shells).
//! This mirrors United Sets' blacklist behavior and surfaces disabled rows in UI.

const BLACKLIST: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "WindowsDashboard",
    "Windows.UI.Core.CoreWindow",
];

/// True if the window class should never be hosted (taskbar, desktop, etc.).
#[inline]
pub fn is_blacklisted(class: &str) -> bool {
    let c = class.trim();
    if c.is_empty() {
        return false;
    }
    let lower = c.to_ascii_lowercase();
    BLACKLIST.iter().any(|b| lower == b.to_ascii_lowercase())
}
