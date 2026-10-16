//! US-hosting controller: groups map to a single host window.
//!
//! Commands flow `UI → Cmd → controller → win → core`. The controller enumerates
//! windows, registers members with the hosting engine, keeps `GroupSet` state,
//! and publishes the current host layout to the reposition worker (spec 01).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use ely_gpui_component::feedback::{Toast, Toaster};
use ely_gpui_component::primitives::Severity;
use gpui::{AppContext, Context, Entity, RenderImage};

/// How long the pointer must rest on a tab before the hover card opens, and how
/// long it may stray before the card closes. Matches ely's `HoverCard`.
const PREVIEW_OPEN_MS: u64 = 500;
const PREVIEW_LINGER_MS: u64 = 200;

use crate::core::{Effect, GroupId, GroupSet, TabId, WindowIdentity};
use crate::win::{
    activate_window, capture_window, close_window, enumerate_windows, install_window_icon,
    is_blacklisted, register_member, unregister_member, window_icon, window_rect, window_title,
    window_under_cursor, Highlight, HostWorker, IconPixels, Janitor, KeyboardCapture, PickKey,
    StylingState, WinEvent, WinEventPump, WindowSnapshot,
};

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
    /// Actually close a hosted window (posts `WM_CLOSE`), as opposed to
    /// `Ungroup`, which only returns it to a standalone window.
    CloseTab {
        group: GroupId,
        tab: TabId,
    },
    ReorderTab {
        group: GroupId,
        from: usize,
        to: usize,
    },
    /// Arm pick mode: hover another app and press TAB to add it.
    BeginPick,
    /// Toggle showing each window's real icon in its tab.
    ToggleIconsInTabs,
    /// A UI overlay (menu) opened/closed; the active member is hidden so it
    /// can't paint over the overlay (owned windows always draw above the host).
    SetOverlay(bool),
    /// Flip the overlay flag from its *current* value. The menu button uses
    /// this rather than `SetOverlay(!captured)`: a captured value goes stale
    /// when clicks outpace re-renders, which desynced the flag from the menu
    /// and could leave the member hidden after rapid toggling.
    ToggleOverlay,
    /// Open/close the tab-overflow dropdown (hides the member while open).
    SetTabMenu(bool),
    /// Flip the tab-overflow dropdown from its *current* value. Like
    /// `ToggleOverlay`, the chevron uses this rather than `SetTabMenu(!captured)`
    /// so a click that outpaces a re-render can't reopen an already-open panel.
    ToggleTabMenu,
    /// Host a specific enumerated window (chosen from the `+` picker).
    AddWindow { hwnd: isize },
    /// Close every tab in the group except `tab`.
    CloseOtherTabs { group: GroupId, tab: TabId },
    /// Close every tab to the right of `tab`.
    CloseTabsToRight { group: GroupId, tab: TabId },
    /// Close every tab to the left of `tab`.
    CloseTabsToLeft { group: GroupId, tab: TabId },
    /// Open the tab right-click menu at a window-space point (hides the member
    /// while open, like the other overlays).
    OpenTabMenu { tab: TabId, at: (f32, f32) },
    CloseTabMenu,
    /// Open the rename prompt for a tab.
    BeginRename { group: GroupId, tab: TabId },
    CancelRename,
    /// Set a tab's display title override (empty clears it).
    RenameTab { group: GroupId, tab: TabId, title: String },
    /// Open/close the `+` window picker (hides the member while open).
    SetPickerOpen(bool),
    TogglePicker,
    /// Show/hide the tab-count badge on the overflow chevron.
    ToggleTabBadge,
    /// Show/hide the hover preview card.
    ToggleHoverPreview,
    /// Hover a tab: after a rest, open the hover preview card under it.
    /// `anchor` is the tab's rectangle in logical client pixels.
    PreviewTab {
        tab: TabId,
        anchor: (f32, f32, f32, f32),
    },
    /// Pointer left `tab`: linger, then hide the card.
    PreviewHide { tab: TabId },
    /// The pointer entered (`true`) or left (`false`) the card itself.
    PreviewCardHover(bool),
    Shutdown,
}

/// Hosting options the user can toggle from the menu.
#[derive(Debug, Clone, Copy)]
pub struct HostSettings {
    pub icons_in_tabs: bool,
    /// Show the tab-count badge on the overflow chevron.
    pub show_tab_badge: bool,
    /// Show the hover preview card when resting on a tab.
    pub hover_preview: bool,
}

impl Default for HostSettings {
    fn default() -> Self {
        Self {
            icons_in_tabs: true,
            show_tab_badge: true,
            hover_preview: true,
        }
    }
}

/// A user-facing transient message. Slate keeps exactly one of these — the
/// pick-mode hint — because a toast painted in the host is covered by the
/// hosted window whenever one is on screen. Other feedback is silent.
fn pick_toast() -> Toast {
    Toast::new("Add a window")
        .severity(Severity::Info)
        .body("Hover a window · press TAB to add · Esc to cancel")
        .stay(Some(Duration::from_secs(4)))
}

/// One tab in the UI-facing snapshot.
#[derive(Clone)]
pub struct TabInfo {
    pub id: TabId,
    pub title: String,
    /// Friendly app name derived from the executable (e.g. "Chrome").
    pub app: String,
    /// The executable file name (e.g. "chrome.exe").
    pub exe: String,
    /// Full path to the executable (for "Copy exe path").
    pub exe_path: String,
    /// The window's real icon, ready to render (present only when extracted).
    pub icon: Option<Arc<RenderImage>>,
}

/// Best-effort friendly app name from an executable file name: strip the
/// extension and upper-case the first letter (`"chrome.exe"` → `"Chrome"`).
fn app_name(exe_name: &str) -> String {
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

/// A UI-facing snapshot of the active host group.
#[derive(Clone, Default)]
pub struct HostInfo {
    pub group: Option<GroupId>,
    pub tabs: Vec<TabInfo>,
    pub active: Option<TabId>,
    pub icons_in_tabs: bool,
    /// Show the tab-count badge on the overflow chevron.
    pub show_tab_badge: bool,
    /// Show the hover preview card when resting on a tab.
    pub hover_preview_enabled: bool,
    /// True while a UI overlay (menu) is open and the member is hidden.
    pub overlay: bool,
    /// True while the tab-overflow dropdown is open and the member is hidden.
    pub tab_menu_open: bool,
    /// True while the `+` picker is open and the member is hidden.
    pub picker_open: bool,
    /// Rows for the `+` picker (running windows + eligibility).
    pub picker_rows: Vec<PickerRow>,
    /// The tab right-click menu's target and window-space position, if open.
    pub tab_menu: Option<TabMenu>,
    /// The tab currently being renamed, if any.
    pub renaming: Option<TabId>,
    /// True while pick mode is armed (the bar shows a spinner).
    pub picking: bool,
    /// The hover preview card to draw over the (now hidden) content area.
    pub hover_preview: Option<HoverPreview>,
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

/// The hover preview card: which tab it mirrors, where it hangs (`anchor`, the
/// tab's rectangle in logical client pixels) and the frame to show. `image` is
/// `None` when the window has never been on screen for us to sample.
#[derive(Clone)]
pub struct HoverPreview {
    pub tab: TabId,
    pub anchor: (f32, f32, f32, f32),
    pub image: Option<Arc<RenderImage>>,
}

pub struct Controller {
    windows: Vec<WindowIdentity>,
    groups: GroupSet,
    active_group: Option<GroupId>,
    host_hwnd: Option<isize>,
    titlebar: f32,
    styling: HashMap<isize, StylingState>,
    worker: HostWorker,
    janitor: Janitor,
    toaster: Entity<Toaster>,
    picking: bool,
    hovered: Option<isize>,
    pick_gen: u64,
    capture: Option<KeyboardCapture>,
    /// Ring drawn over the window the pointer is on while picking.
    highlight: Option<Highlight>,
    settings: HostSettings,
    icons: HashMap<isize, Option<Arc<RenderImage>>>,
    overlay: bool,
    /// True while the tab-overflow dropdown is open; like `overlay`, it hides the
    /// active member so the dropdown isn't covered by the owned window.
    tab_menu_open: bool,
    /// True while the `+` picker is open (hides the member, same reason).
    picker_open: bool,
    /// Cached rows for the `+` picker, rebuilt each time it opens.
    picker_rows: Vec<PickerRow>,
    /// The tab right-click menu's target and position, if open.
    tab_menu: Option<TabMenu>,
    /// The tab being renamed, if any (opens the rename prompt).
    renaming: Option<(GroupId, TabId)>,
    /// Display-title overrides for renamed tabs, keyed by hwnd. Takes precedence
    /// over the live window title and survives retitle events.
    rename_overrides: HashMap<isize, String>,
    /// The hover preview card, while shown. Rendering it hides the member (same
    /// as a menu), so the card is a snapshot captured before the member hid.
    hover_preview: Option<HoverPreview>,
    /// The tab the pointer is over right now, whether or not the card is up.
    hover_tab: Option<TabId>,
    /// True while the pointer is over the card itself, which keeps it open.
    card_hovered: bool,
    /// Bumped on every hover change to cancel stale open/linger timers.
    preview_gen: u64,
    /// Last captured frame per window, so a tab that isn't current (its window
    /// is hidden, nothing to sample) still shows something.
    snapshots: HashMap<isize, Arc<RenderImage>>,
    _pump: Option<WinEventPump>,
}

impl Controller {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let toaster = cx.new(|_| Toaster::default());
        let controller = Self {
            windows: enumerate_windows(),
            groups: GroupSet::new(),
            active_group: None,
            host_hwnd: None,
            titlebar: 0.0,
            styling: HashMap::new(),
            worker: HostWorker::new(),
            janitor: Janitor::new(),
            picking: false,
            hovered: None,
            pick_gen: 0,
            capture: None,
            highlight: None,
            settings: HostSettings::default(),
            icons: HashMap::new(),
            overlay: false,
            tab_menu_open: false,
            picker_open: false,
            picker_rows: Vec::new(),
            tab_menu: None,
            renaming: None,
            rename_overrides: HashMap::new(),
            hover_preview: None,
            hover_tab: None,
            card_hovered: false,
            preview_gen: 0,
            snapshots: HashMap::new(),
            _pump: WinEventPump::install(),
            toaster,
        };
        Self::start_event_pump(cx);
        controller
    }

    /// Drain OS window events that arrive while the user does nothing inside
    /// Slate — a hosted window retitles itself (e.g. a browser switches tabs) or
    /// closes. Commands already drain on `dispatch`; this keeps the strip live
    /// between commands instead of only after the next click.
    fn start_event_pump(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
            if this
                .update(cx, |controller, cx| controller.tick(cx))
                .is_err()
            {
                break;
            }
        })
        .detach();
    }

    /// One drain of the WinEvent queue; re-renders only when something changed.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.drain_events() {
            self.sync_worker();
            cx.notify();
        }
    }

    /// Snapshot for rendering the tab strip.
    pub fn host_info(&self) -> HostInfo {
        let group = self.active_group.and_then(|id| self.groups.group(id));
        HostInfo {
            group: group.map(|g| g.id),
            tabs: group
                .map(|g| {
                    g.tabs
                        .iter()
                        .map(|t| TabInfo {
                            id: t.id,
                            title: self
                                .rename_overrides
                                .get(&t.window.hwnd)
                                .cloned()
                                .unwrap_or_else(|| t.window.title.clone()),
                            app: app_name(&t.window.exe_name),
                            exe: t.window.exe_name.clone(),
                            exe_path: t.window.exe_path.clone(),
                            icon: self.icons.get(&t.window.hwnd).cloned().flatten(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            active: group.and_then(|g| g.active),
            icons_in_tabs: self.settings.icons_in_tabs,
            show_tab_badge: self.settings.show_tab_badge,
            hover_preview_enabled: self.settings.hover_preview,
            overlay: self.overlay,
            tab_menu_open: self.tab_menu_open,
            picker_open: self.picker_open,
            picker_rows: self.picker_rows.clone(),
            tab_menu: self.tab_menu.clone(),
            renaming: self.renaming.map(|(_, tab)| tab),
            picking: self.picking,
            hover_preview: self.hover_preview.clone(),
        }
    }

    /// The toaster the host view renders the pick hint through. Shared so the
    /// view can observe arrivals/expiries and re-render.
    pub fn toaster(&self) -> Entity<Toaster> {
        self.toaster.clone()
    }

    /// Apply a command. Called from the UI thread; `cx` is the controller's
    /// context so the host view re-renders after every change.
    pub fn dispatch(&mut self, cmd: Cmd, cx: &mut Context<Self>) {
        self.drain_events();
        match cmd {
            Cmd::Refresh => self.windows = enumerate_windows(),
            Cmd::HostReady { hwnd, titlebar } => {
                self.host_hwnd = Some(hwnd);
                self.titlebar = titlebar;
                install_window_icon(hwnd);
                self.worker.start();
                self.worker.install_move_tracker(hwnd);
            }
            Cmd::SelectTab { group, tab } => self.select_tab(group, tab),
            Cmd::Ungroup { group, tab } => self.ungroup(group, tab),
            Cmd::CloseTab { group, tab } => self.close_tab(group, tab),
            Cmd::ReorderTab { group, from, to } => self.reorder(group, from, to),
            Cmd::BeginPick => self.begin_pick(cx),
            Cmd::ToggleIconsInTabs => self.toggle_icons_in_tabs(),
            Cmd::SetOverlay(open) => self.overlay = open,
            Cmd::ToggleOverlay => self.overlay = !self.overlay,
            Cmd::SetTabMenu(open) => {
                self.tab_menu_open = open;
                if open {
                    self.picker_open = false;
                    self.tab_menu = None;
                }
            }
            Cmd::ToggleTabMenu => {
                self.tab_menu_open = !self.tab_menu_open;
                if self.tab_menu_open {
                    self.picker_open = false;
                    self.tab_menu = None;
                }
            }
            Cmd::AddWindow { hwnd } => self.add_window(hwnd),
            Cmd::CloseOtherTabs { group, tab } => self.close_other_tabs(group, tab),
            Cmd::CloseTabsToRight { group, tab } => self.close_tabs_to_right(group, tab),
            Cmd::CloseTabsToLeft { group, tab } => self.close_tabs_to_left(group, tab),
            Cmd::OpenTabMenu { tab, at } => {
                self.tab_menu = Some(TabMenu { tab, at });
                self.tab_menu_open = false;
                self.picker_open = false;
            }
            Cmd::CloseTabMenu => self.tab_menu = None,
            Cmd::BeginRename { group, tab } => {
                self.tab_menu = None;
                self.renaming = Some((group, tab));
            }
            Cmd::CancelRename => self.renaming = None,
            Cmd::RenameTab { group, tab, title } => self.rename_tab(group, tab, title),
            Cmd::SetPickerOpen(open) => self.set_picker_open(open),
            Cmd::TogglePicker => {
                let open = !self.picker_open;
                self.set_picker_open(open);
            }
            Cmd::ToggleTabBadge => self.settings.show_tab_badge = !self.settings.show_tab_badge,
            Cmd::ToggleHoverPreview => self.toggle_hover_preview(),
            Cmd::PreviewTab { tab, anchor } => self.preview_hover(tab, anchor, cx),
            Cmd::PreviewHide { tab } => self.preview_unhover(tab, cx),
            Cmd::PreviewCardHover(hovered) => self.preview_card_hover(hovered, cx),
            Cmd::Shutdown => self.shutdown(),
        }
        self.sync_worker();
        cx.notify();
    }

    /// Restore every hosted window to normal, then stop the worker. Safe to
    /// call more than once; never leaves a member hidden (spec 02/06).
    fn shutdown(&mut self) {
        self.end_pick();
        self.hide_hover_preview();
        // Remember the member that was on screen so it can be re-focused once it
        // is restored: a restored window is missing from the taskbar and unfocused
        // until re-activated, which reads as "the window didn't come back".
        let restore_focus = self.active_group.and_then(|g| self.active_hwnd(g));
        for (hwnd, state) in self.styling.drain() {
            unregister_member(hwnd, &state);
        }
        if let Some(hwnd) = restore_focus {
            activate_window(hwnd);
        }
        self.groups = GroupSet::new();
        self.active_group = None;
        self.host_hwnd = None;
        self.overlay = false;
        self.tab_menu_open = false;
        self.picker_open = false;
        self.picker_rows.clear();
        self.tab_menu = None;
        self.renaming = None;
        self.rename_overrides.clear();
        self.worker.stop();
    }

    /// Arm pick mode: refresh the window list, capture TAB/Esc system-wide, and
    /// poll the cursor until the user commits or cancels (spec 02).
    fn begin_pick(&mut self, cx: &mut Context<Self>) {
        self.windows = enumerate_windows();
        self.end_pick();
        self.picking = true;
        self.capture = KeyboardCapture::install();
        if self.highlight.is_none() {
            self.highlight = Highlight::new();
        }
        self.toaster.update(cx, |toaster, cx| {
            toaster.push(pick_toast(), cx);
        });
        self.pick_gen = self.pick_gen.wrapping_add(1);
        let generation = self.pick_gen;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            let done = this
                .update(cx, |controller, cx| controller.poll_pick(generation, cx))
                .unwrap_or(true);
            if done {
                break;
            }
        })
        .detach();
    }

    /// One polling tick of pick mode. Returns true once the pick has ended.
    fn poll_pick(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.picking || generation != self.pick_gen {
            return true;
        }
        let keys = self
            .capture
            .as_ref()
            .map(KeyboardCapture::drain)
            .unwrap_or_default();
        if let Some(key) = keys.into_iter().next() {
            match key {
                PickKey::Tab => self.commit_pick(),
                PickKey::Escape => self.end_pick(),
            }
            self.sync_worker();
            cx.notify();
            return true;
        }
        let hovered =
            window_under_cursor().filter(|hwnd| self.windows.iter().any(|w| w.hwnd == *hwnd));
        if self.hovered != hovered {
            self.hovered = hovered;
            self.update_highlight();
            cx.notify();
        }
        false
    }

    /// Move the pick ring onto the hovered window (or hide it when nothing
    /// eligible is under the cursor).
    fn update_highlight(&self) {
        let Some(highlight) = self.highlight.as_ref() else {
            return;
        };
        match self.hovered.and_then(window_rect) {
            Some((x, y, w, h)) => highlight.show(x, y, w, h),
            None => highlight.hide(),
        }
    }

    /// Add the window the pick settled on, if any, then leave pick mode.
    fn commit_pick(&mut self) {
        let target = self.hovered.take();
        self.end_pick();
        if let Some(hwnd) = target {
            self.add_window(hwnd);
        }
    }

    /// Leave pick mode and drop the keyboard hook. Leaves no status behind.
    fn end_pick(&mut self) {
        self.picking = false;
        self.hovered = None;
        self.capture = None;
        if let Some(highlight) = self.highlight.as_ref() {
            highlight.hide();
        }
        // Invalidate any poll task still waiting on a timer.
        self.pick_gen = self.pick_gen.wrapping_add(1);
    }

    /// Register a running window as a tab in the active group.
    fn add_window(&mut self, hwnd: isize) {
        let Some(host) = self.host_hwnd else {
            return;
        };
        let Some(identity) = self.windows.iter().find(|w| w.hwnd == hwnd).cloned() else {
            return;
        };
        if is_blacklisted(&identity.class) {
            return;
        }
        if identity.elevated {
            return;
        }
        if self.groups.contains_window(hwnd) {
            return;
        }
        let group = match self.active_group {
            Some(id) => id,
            None => {
                let id = self.groups.create_group("Slate", Vec::new());
                self.active_group = Some(id);
                id
            }
        };
        if let Some(state) = register_member(host, hwnd) {
            self.styling.insert(hwnd, state);
        } else {
            return;
        }
        if self.settings.icons_in_tabs {
            self.ensure_icon(hwnd);
        }
        self.groups.add_window(group, identity);
        if let Some((_, tab)) = self.tab_of_hwnd(hwnd) {
            self.groups.set_active(group, tab);
            self.activate_member(hwnd);
        }
    }

    /// Show the selected tab's member; siblings stay hidden.
    fn select_tab(&mut self, group: GroupId, tab: TabId) {
        self.groups.set_active(group, tab);
        if let Some(hwnd) = self.hwnd_of_tab(group, tab) {
            self.activate_member(hwnd);
        }
    }

    /// Remove a tab and restore its window to normal. Never closes the app.
    fn ungroup(&mut self, group: GroupId, tab: TabId) {
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        if let Some(state) = self.styling.remove(&hwnd) {
            unregister_member(hwnd, &state);
        }
        self.icons.remove(&hwnd);
        self.rename_overrides.remove(&hwnd);
        self.groups.remove_tab(group, tab);
    }

    /// Really close a hosted window: post `WM_CLOSE` and let the app quit on
    /// its own terms. The member is first released so the host stops pinning
    /// it; the resulting destroy event then clears the tab via `window_gone`.
    fn close_tab(&mut self, group: GroupId, tab: TabId) {
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        if let Some(state) = self.styling.remove(&hwnd) {
            unregister_member(hwnd, &state);
        }
        close_window(hwnd);
    }

    /// Reorder a tab within its group by index.
    fn reorder(&mut self, group: GroupId, from: usize, to: usize) {
        let Some(tab) = self
            .groups
            .group(group)
            .and_then(|g| g.tabs.get(from))
            .map(|t| t.id)
        else {
            return;
        };
        self.groups.reorder(group, tab, to);
    }

    /// Close every tab in the group except `keep` (each window is restored).
    fn close_other_tabs(&mut self, group: GroupId, keep: TabId) {
        let Some(g) = self.groups.group(group) else {
            return;
        };
        let ids: Vec<TabId> = g.tabs.iter().map(|t| t.id).filter(|id| *id != keep).collect();
        for id in ids {
            self.ungroup(group, id);
        }
    }

    /// Close every tab positioned to the right of `tab`.
    fn close_tabs_to_right(&mut self, group: GroupId, tab: TabId) {
        let Some(g) = self.groups.group(group) else {
            return;
        };
        let Some(ix) = g.tabs.iter().position(|t| t.id == tab) else {
            return;
        };
        let ids: Vec<TabId> = g.tabs.iter().skip(ix + 1).map(|t| t.id).collect();
        for id in ids {
            self.ungroup(group, id);
        }
    }

    /// Close every tab positioned to the left of `tab`.
    fn close_tabs_to_left(&mut self, group: GroupId, tab: TabId) {
        let Some(g) = self.groups.group(group) else {
            return;
        };
        let Some(ix) = g.tabs.iter().position(|t| t.id == tab) else {
            return;
        };
        let ids: Vec<TabId> = g.tabs.iter().take(ix).map(|t| t.id).collect();
        for id in ids {
            self.ungroup(group, id);
        }
    }

    /// Set (or clear, when empty) a tab's display-title override.
    fn rename_tab(&mut self, group: GroupId, tab: TabId, title: String) {
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        let trimmed = title.trim();
        if trimmed.is_empty() {
            self.rename_overrides.remove(&hwnd);
        } else {
            self.rename_overrides.insert(hwnd, trimmed.to_string());
        }
        self.renaming = None;
    }

    /// Open or close the `+` picker, rebuilding its rows on open.
    fn set_picker_open(&mut self, open: bool) {
        self.picker_open = open;
        if open {
            // Only one dropdown is open at a time.
            self.tab_menu_open = false;
            self.tab_menu = None;
            self.refresh_picker();
        }
    }

    /// Rebuild the picker rows: eligible windows first, each flagged with a
    /// reason when it can't be hosted. Extracts icons for eligible rows.
    fn refresh_picker(&mut self) {
        let hosted: Vec<isize> = self.styling.keys().copied().collect();
        let eligible: Vec<isize> = self
            .windows
            .iter()
            .filter(|w| !is_blacklisted(&w.class) && !w.elevated && !hosted.contains(&w.hwnd))
            .map(|w| w.hwnd)
            .collect();
        for hwnd in eligible {
            self.ensure_icon(hwnd);
        }
        let mut rows: Vec<PickerRow> = self
            .windows
            .iter()
            .map(|w| {
                let reason = if is_blacklisted(&w.class) {
                    Some("This window can't be hosted".to_string())
                } else if w.elevated {
                    Some("Run Slate as administrator to host this".to_string())
                } else if hosted.contains(&w.hwnd) {
                    Some("Already hosted".to_string())
                } else {
                    None
                };
                PickerRow {
                    hwnd: w.hwnd,
                    title: if w.title.is_empty() {
                        w.exe_name.clone()
                    } else {
                        w.title.clone()
                    },
                    exe: w.exe_name.clone(),
                    icon: self.icons.get(&w.hwnd).cloned().flatten(),
                    reason,
                }
            })
            .collect();
        // Eligible first, then alphabetically; stable within each bucket.
        rows.sort_by(|a, b| {
            a.reason
                .is_some()
                .cmp(&b.reason.is_some())
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        self.picker_rows = rows;
    }

    /// True while any UI surface hides the hosted member: the menu, the
    /// dropdowns, the picker, the rename prompt, or the hover card. An owned
    /// member paints above the host, so these are the states that blank the
    /// content rect.
    fn overlaying(&self) -> bool {
        self.menu_overlay() || self.hover_preview.is_some()
    }

    /// The menu-ish overlays only — everything that hides the member for a
    /// reason other than the hover card itself.
    fn menu_overlay(&self) -> bool {
        self.overlay
            || self.tab_menu_open
            || self.picker_open
            || self.tab_menu.is_some()
            || self.renaming.is_some()
    }

    /// The pointer rested on a tab: open the card under it, on ely's
    /// `HoverCard` rhythm (500 ms to open, 200 ms to close). While the card is
    /// already up, moving to another tab switches it with no delay.
    fn preview_hover(
        &mut self,
        tab: TabId,
        anchor: (f32, f32, f32, f32),
        cx: &mut Context<Self>,
    ) {
        if !self.settings.hover_preview {
            return;
        }
        self.hover_tab = Some(tab);
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if self.hover_preview.is_some() {
            self.show_hover_preview(tab, anchor);
            return;
        }
        self.after_hover_delay(PREVIEW_OPEN_MS, cx, move |c, cx| {
            c.show_hover_preview(tab, anchor);
            c.sync_worker();
            cx.notify();
        });
    }

    /// Pointer left `tab`: linger so the card can be crossed, then drop it. A
    /// stale leave (the pointer is already on another tab) is ignored, so
    /// switching tabs never blanks the card.
    fn preview_unhover(&mut self, tab: TabId, cx: &mut Context<Self>) {
        if self.hover_tab != Some(tab) {
            return;
        }
        self.hover_tab = None;
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if self.hover_preview.is_none() {
            return;
        }
        self.after_hover_delay(PREVIEW_LINGER_MS, cx, |c, cx| {
            // The pointer may have crossed into the card, which keeps it open.
            if !c.card_hovered {
                c.hide_hover_preview();
                c.sync_worker();
                cx.notify();
            }
        });
    }

    /// The pointer entered (`true`) or left (`false`) the card. Entering
    /// cancels a pending hide so crossing the gap from the tab into the card
    /// keeps it up; leaving starts the linger, unless the pointer is back on a
    /// tab (whose own hover takes over).
    fn preview_card_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.card_hovered = hovered;
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if hovered || self.hover_preview.is_none() || self.hover_tab.is_some() {
            return;
        }
        self.after_hover_delay(PREVIEW_LINGER_MS, cx, |c, cx| {
            if !c.card_hovered && c.hover_tab.is_none() {
                c.hide_hover_preview();
                c.sync_worker();
                cx.notify();
            }
        });
    }

    /// Show/hide the hover preview card. Hiding it also drops any card on
    /// screen and forgets the hover state.
    fn toggle_hover_preview(&mut self) {
        self.settings.hover_preview = !self.settings.hover_preview;
        if !self.settings.hover_preview {
            self.hover_tab = None;
            self.card_hovered = false;
            self.preview_gen = self.preview_gen.wrapping_add(1);
            self.hide_hover_preview();
        }
    }

    /// Run `f` after `ms`, unless a later hover change bumps the generation
    /// first — which cancels this pending open or linger.
    fn after_hover_delay(
        &mut self,
        ms: u64,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let gen = self.preview_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(ms))
                .await;
            let _ = this.update(cx, |c, cx| {
                if c.preview_gen == gen {
                    f(c, cx);
                }
            });
        })
        .detach();
    }

    /// Show the card. The frame is sampled here — before opening, which hides
    /// the member — and only for the active window, since the inactive ones are
    /// already hidden; those fall back to the last frame we caught.
    fn show_hover_preview(&mut self, tab: TabId, anchor: (f32, f32, f32, f32)) {
        let (Some(_host), Some(group)) = (self.host_hwnd, self.active_group) else {
            return;
        };
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        let active = self.active_hwnd(group);
        let image = if Some(hwnd) == active && !self.overlaying() {
            let captured = capture_window(hwnd).and_then(render_snapshot);
            if let Some(frame) = captured.as_ref() {
                self.snapshots.insert(hwnd, frame.clone());
            }
            captured.or_else(|| self.snapshots.get(&hwnd).cloned())
        } else {
            self.snapshots.get(&hwnd).cloned()
        };
        self.hover_preview = Some(HoverPreview {
            tab,
            anchor,
            image,
        });
    }

    /// Drop the card; the member comes back on the next `sync_worker`.
    fn hide_hover_preview(&mut self) {
        self.hover_preview = None;
        self.card_hovered = false;
    }

    /// A destroyed foreign window is no longer hostable: drop its tab.
    fn window_gone(&mut self, hwnd: isize) {
        self.styling.remove(&hwnd);
        self.icons.remove(&hwnd);
        self.snapshots.remove(&hwnd);
        self.rename_overrides.remove(&hwnd);
        if let Some(tab) = self.hover_preview.as_ref().map(|p| p.tab) {
            if self.active_group.and_then(|g| self.hwnd_of_tab(g, tab)) == Some(hwnd) {
                self.hide_hover_preview();
            }
        }
        self.windows.retain(|w| w.hwnd != hwnd);
        self.groups.window_destroyed(hwnd);
        if self.picker_open {
            self.refresh_picker();
        }
    }

    /// Show/hide the real window icon in each tab, extracting on first enable.
    fn toggle_icons_in_tabs(&mut self) {
        self.settings.icons_in_tabs = !self.settings.icons_in_tabs;
        if self.settings.icons_in_tabs {
            for hwnd in self.styling.keys().copied().collect::<Vec<_>>() {
                self.ensure_icon(hwnd);
            }
        }
    }

    /// Extract and cache a window's icon the first time it is needed.
    fn ensure_icon(&mut self, hwnd: isize) {
        if self.icons.contains_key(&hwnd) {
            return;
        }
        let icon = window_icon(hwnd).and_then(render_image);
        self.icons.insert(hwnd, icon);
    }

    /// Absorb WinEvent callbacks queued on the UI thread. Returns true if any
    /// event changed state, so the caller can re-render.
    fn drain_events(&mut self) -> bool {
        let mut changed = false;
        for event in WinEventPump::drain() {
            // The hooks are process-wide, so events arrive for every window on
            // the system. Only hosted members matter; skip the rest before any
            // title/identity lookups.
            match event {
                WinEvent::Destroyed(hwnd) if self.styling.contains_key(&hwnd) => {
                    self.window_gone(hwnd);
                    changed = true;
                }
                WinEvent::TitleChanged(hwnd)
                    if self.styling.contains_key(&hwnd)
                        && !self.rename_overrides.contains_key(&hwnd) =>
                {
                    // A renamed tab keeps its override; the live title is ignored.
                    let title = window_title(hwnd);
                    if self.groups.update_title(hwnd, &title) == Effect::Applied {
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        changed
    }

    /// Bring a member forward and give it focus so typing works.
    fn activate_member(&mut self, hwnd: isize) {
        activate_window(hwnd);
    }

    /// Publish the host layout (active member + hidden siblings) to the worker.
    fn sync_worker(&mut self) {
        let host = self.host_hwnd.unwrap_or(0);
        // A menu, dropdown, or picker opening over the card wins: drop the card.
        // (Only the menu-ish surfaces — not the card itself.)
        if self.menu_overlay() {
            self.hide_hover_preview();
        } else if let Some(tab) = self.hover_preview.as_ref().map(|p| p.tab) {
            let hosted = self
                .active_group
                .and_then(|g| self.hwnd_of_tab(g, tab))
                .is_some_and(|h| self.styling.contains_key(&h));
            if !hosted {
                self.hide_hover_preview();
            }
        }
        // While any overlay (menu, dropdown, or card) is up, hide the active
        // member too: an owned window always paints above the host.
        let overlaying = self.overlaying();
        let active = if overlaying {
            None
        } else {
            self.active_group.and_then(|g| self.active_hwnd(g))
        };
        let hidden: Vec<isize> = self
            .styling
            .keys()
            .copied()
            .filter(|h| Some(*h) != active)
            .collect();
        self.worker.update(host, self.titlebar, active, hidden);
        // Hand the current membership to the crash watchdog so a force-kill can
        // still restore every hosted window (spec 06).
        let members: Vec<(isize, StylingState)> =
            self.styling.iter().map(|(k, v)| (*k, v.clone())).collect();
        self.janitor.persist(host, &members);
    }

    fn active_hwnd(&self, group: GroupId) -> Option<isize> {
        let g = self.groups.group(group)?;
        let tab = g.active?;
        self.hwnd_of_tab(group, tab)
    }

    fn hwnd_of_tab(&self, group: GroupId, tab: TabId) -> Option<isize> {
        self.groups
            .group(group)?
            .tabs
            .iter()
            .find(|t| t.id == tab)
            .map(|t| t.window.hwnd)
    }

    fn tab_of_hwnd(&self, hwnd: isize) -> Option<(GroupId, TabId)> {
        self.groups.groups().iter().find_map(|g| {
            g.tabs
                .iter()
                .find(|t| t.window.hwnd == hwnd)
                .map(|t| (g.id, t.id))
        })
    }
}

/// Decode raw RGBA pixels into a gpui image, once per window.
fn render_image(px: IconPixels) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(px.width, px.height, px.rgba)?;
    Some(crate::ui::render_image(buffer))
}

/// Wrap a captured window frame as a gpui image.
fn render_snapshot(snap: WindowSnapshot) -> Option<Arc<RenderImage>> {
    let buffer = image::RgbaImage::from_raw(snap.width, snap.height, snap.rgba)?;
    Some(crate::ui::render_image(buffer))
}
