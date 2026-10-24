use ely_gpui_component::feedback::Toast;
use ely_gpui_component::primitives::Severity;
use gpui::{RenderImage, SharedString};
use std::sync::Arc;
use std::time::Duration;

use crate::core::{GroupId, TabId, UpdateState};
use crate::win::{IconPixels, WindowSnapshot};

#[derive(Debug, Clone)]
pub enum Cmd {
    Refresh,
    HostReady {
        hwnd: isize,
        titlebar: f32,
    },
    SelectTab {
        group: GroupId,
        tab: TabId,
    },
    Ungroup {
        group: GroupId,
        tab: TabId,
    },
    CloseTab {
        group: GroupId,
        tab: TabId,
    },
    DetachTabOut {
        from: usize,
        screen_x: f32,
        screen_y: f32,
    },
    ReorderTab {
        group: GroupId,
        from: usize,
        to: usize,
    },
    BeginPick,
    ToggleIconsInTabs,
    SetOverlay(bool),
    ToggleOverlay,
    SetTabMenu(bool),
    ToggleTabMenu,
    AddWindow {
        hwnd: isize,
    },
    CloseOtherTabs {
        group: GroupId,
        tab: TabId,
    },
    CloseTabsToRight {
        group: GroupId,
        tab: TabId,
    },
    CloseTabsToLeft {
        group: GroupId,
        tab: TabId,
    },
    OpenTabMenu {
        tab: TabId,
        at: (f32, f32),
    },
    CloseTabMenu,
    BeginRename {
        group: GroupId,
        tab: TabId,
    },
    CancelRename,
    RenameTab {
        group: GroupId,
        tab: TabId,
        title: String,
    },
    SetPickerOpen(bool),
    TogglePicker,
    SetSavedMenu(bool),
    ToggleSavedMenu,
    BeginSaveSet,
    BeginRenameSet {
        id: u64,
    },
    CancelSavedPrompt,
    CommitSaveSet {
        name: String,
    },
    CommitRenameSet {
        id: u64,
        name: String,
    },
    RestoreSet {
        id: u64,
    },
    DeleteSet {
        id: u64,
    },
    ToggleTabBadge,
    ToggleHoverPreview,
    ToggleTheme,
    SetTabColor {
        group: GroupId,
        tab: TabId,
        color: Option<usize>,
    },
    PreviewTab {
        tab: TabId,
        anchor: (f32, f32, f32, f32),
    },
    PreviewHide {
        tab: TabId,
    },
    PreviewCardHover(bool),
    Shutdown,
    ToggleTabBar,
    OpenRepository,
    CheckForUpdates,
    OpenSlateFolder,
}

#[derive(Clone)]
pub struct TabInfo {
    pub id: TabId,
    pub title: String,
    pub app: String,
    pub exe: String,
    pub exe_path: String,
    pub icon: Option<Arc<RenderImage>>,
    pub accent: Option<usize>,
}

#[derive(Clone)]
pub struct HostInfo {
    pub group: Option<GroupId>,
    pub tabs: Vec<TabInfo>,
    pub active: Option<TabId>,
    pub icons_in_tabs: bool,
    pub show_tab_badge: bool,
    pub hover_preview_enabled: bool,
    pub overlay: bool,
    pub tab_menu_open: bool,
    pub picker_open: bool,
    pub picker_rows: Vec<PickerRow>,
    pub saved_menu_open: bool,
    pub saved_sets: Vec<SavedSetInfo>,
    pub saved_prompt: Option<SavedPrompt>,
    pub tab_menu: Option<TabMenu>,
    pub renaming: Option<TabId>,
    pub picking: bool,
    pub hover_preview: Option<HoverPreview>,
    pub show_tab_bar: bool,
    pub update: UpdateState,
    pub caption: SharedString,
    pub version_label: SharedString,
    pub update_label: SharedString,
}

#[derive(Clone)]
pub struct TabMenu {
    pub tab: TabId,
    pub at: (f32, f32),
}

#[derive(Clone)]
pub struct PickerRow {
    pub hwnd: isize,
    pub title: String,
    pub exe: String,
    pub icon: Option<Arc<RenderImage>>,
    pub reason: Option<String>,
}

#[derive(Clone)]
pub struct SavedSetInfo {
    pub id: u64,
    pub name: String,
    pub available: usize,
    pub total: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SavedPrompt {
    Save,
    Rename(u64),
}

#[derive(Clone)]
pub struct HoverPreview {
    pub tab: TabId,
    pub anchor: (f32, f32, f32, f32),
    pub image: Option<Arc<RenderImage>>,
}

pub(super) fn pick_toast() -> Toast {
    Toast::new("Add a window")
        .severity(Severity::Info)
        .body("Hover a window - press TAB to add - Esc to cancel")
        .stay(Some(Duration::from_secs(4)))
}

pub(super) fn update_toast(
    title: &'static str,
    severity: Severity,
    body: impl Into<SharedString>,
) -> Toast {
    Toast::new(title)
        .severity(severity)
        .body(body)
        .stay(Some(Duration::from_secs(5)))
}

pub(super) fn app_name(exe_name: &str) -> String {
    let stem = exe_name
        .strip_suffix(".exe")
        .or_else(|| exe_name.strip_suffix(".EXE"))
        .unwrap_or(exe_name)
        .trim();
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => exe_name.to_string(),
    }
}

pub(super) fn render_image(px: IconPixels) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(px.width, px.height, px.bgra)?;
    Some(crate::ui::render_image(buffer))
}

pub(super) fn render_snapshot(snap: WindowSnapshot) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(snap.width, snap.height, snap.bgra)?;
    Some(crate::ui::render_image(buffer))
}
