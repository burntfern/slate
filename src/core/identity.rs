use serde::{Deserialize, Serialize};

/// Stable per-run identity of a grouped application window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowIdentity {
    pub hwnd: isize,
    pub pid: u32,
    pub exe_path: String,
    pub exe_name: String,
    pub title: String,
    pub class: String,
    pub elevated: bool,
}

/// Identity of a tab inside a group. Independent of the window it points at
/// so UI state (rename, order) survives a window being closed and reopened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

/// Identity of a group of tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GroupId(pub u64);
