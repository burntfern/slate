const BLACKLIST: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "WindowsDashboard",
    "Windows.UI.Core.CoreWindow",
];

#[inline]
pub fn is_blacklisted(class: &str) -> bool {
    let c = class.trim();
    if c.is_empty() {
        return false;
    }
    let lower = c.to_ascii_lowercase();
    BLACKLIST.iter().any(|b| lower == b.to_ascii_lowercase())
}
