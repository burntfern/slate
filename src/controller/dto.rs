use ely_gpui_component::feedback::Toast;
use ely_gpui_component::primitives::Severity;
use gpui::RenderImage;
use std::sync::Arc;
use std::time::Duration;

use crate::core::{GroupId, TabId};
use crate::win::{IconPixels, WindowSnapshot};

/// UI → controller intent. The UI never calls Win32 directly.
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
    /// Post `WM_CLOSE` to the hosted window, as opposed to `Ungroup`, which only
    /// returns it to a standalone window.
    CloseTab {
        group: GroupId,
        tab: TabId,
    },
    /// Drag a tab out and drop it over the content area: ungroup it and move the
    /// restored window to the drop's screen position.
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
    /// Flip the overlay flag from its *current* value. The menu button uses this
    /// rather than `SetOverlay(!captured)`: a captured value goes stale when
    /// clicks outpace re-renders, desyncing the flag from the menu.
    ToggleOverlay,
    SetTabMenu(bool),
    /// Flip the tab-overflow dropdown from its *current* value, for the same
    /// staleness reason as `ToggleOverlay`.
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
        index: usize,
    },
    CancelSavedPrompt,
    CommitSaveSet {
        name: String,
    },
    CommitRenameSet {
        index: usize,
        name: String,
    },
    RestoreSet {
        index: usize,
    },
    DeleteSet {
        index: usize,
    },
    ToggleTabBadge,
    ToggleHoverPreview,
    ToggleTheme,
    /// Session-only accent (chart-hue index); forgotten on restart.
    SetTabColor {
        group: GroupId,
        tab: TabId,
        color: Option<usize>,
    },
    /// Hover a tab: after a rest, open the preview card under it. `anchor` is the
    /// tab's rectangle in logical client pixels.
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
}

/// One tab in the UI-facing snapshot.
#[derive(Clone)]
pub struct TabInfo {
    pub id: TabId,
    pub title: String,
    /// Friendly app name derived from the executable (e.g. "Chrome").
    pub app: String,
    pub exe: String,
    pub exe_path: String,
    pub icon: Option<Arc<RenderImage>>,
    /// Chart-hue index (see `HUE_NAMES`), or `None` for the plain default tab.
    pub accent: Option<usize>,
}

/// A UI-facing snapshot of the active host group.
#[derive(Clone, Default)]
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
}

/// The tab right-click menu: which tab, and where it was requested.
#[derive(Clone)]
pub struct TabMenu {
    pub tab: TabId,
    pub at: (f32, f32),
}

/// One row of the `+` window picker.
#[derive(Clone)]
pub struct PickerRow {
    pub hwnd: isize,
    pub title: String,
    pub exe: String,
    pub icon: Option<Arc<RenderImage>>,
    /// `Some` when the window can't be hosted; the string is the reason.
    pub reason: Option<String>,
}

/// One row of the Saved-tab-sets panel.
#[derive(Clone)]
pub struct SavedSetInfo {
    pub name: String,
    pub count: usize,
    /// Tabs with a matching window running right now (and not already hosted).
    /// The rest are "closed" — attach-only, so they won't open.
    pub available: usize,
}

/// The Saved-tab-sets inline name prompt: saving the current tabs as a new set,
/// or renaming the set at `index`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SavedPrompt {
    Save,
    Rename(usize),
}

/// The hover preview card: the tab it mirrors, its `anchor` rectangle in logical
/// client pixels, and the frame to show (`None` if never on screen to sample).
#[derive(Clone)]
pub struct HoverPreview {
    pub tab: TabId,
    pub anchor: (f32, f32, f32, f32),
    pub image: Option<Arc<RenderImage>>,
}

/// The single pick-mode toast. Other feedback is silent: a toast painted in the
/// host is covered by the hosted window whenever one is on screen.
pub(super) fn pick_toast() -> Toast {
    Toast::new("Add a window")
        .severity(Severity::Info)
        .body("Hover a window · press TAB to add · Esc to cancel")
        .stay(Some(Duration::from_secs(4)))
}

/// Best-effort friendly app name from an executable file name: strip the
/// extension and upper-case the first letter (`"chrome.exe"` → `"Chrome"`).
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

/// Decode raw RGBA pixels into a gpui image, once per window.
pub(super) fn render_image(px: IconPixels) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(px.width, px.height, px.rgba)?;
    Some(crate::ui::render_image(buffer))
}

/// Wrap a captured window frame as a gpui image.
pub(super) fn render_snapshot(snap: WindowSnapshot) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(snap.width, snap.height, snap.rgba)?;
    Some(crate::ui::render_image(buffer))
}
