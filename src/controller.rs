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

use crate::core::{Effect, GroupId, GroupSet, TabId, WindowIdentity};
use crate::win::{
    activate_window, enumerate_windows, install_window_icon, is_blacklisted, register_member,
    unregister_member, window_icon, window_rect, window_title, window_under_cursor, Highlight,
    HostWorker, IconPixels, Janitor, KeyboardCapture, PickKey, StylingState, WinEvent,
    WinEventPump,
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
    /// Place the theme toggle button on the left (next to the app menu) or the
    /// right (after the tab-overflow chevron), just before the window controls.
    SetThemeButtonRight(bool),
    Shutdown,
}

/// Hosting options the user can toggle from the menu.
#[derive(Debug, Clone, Copy)]
pub struct HostSettings {
    pub icons_in_tabs: bool,
    /// Theme toggle on the right of the title bar (after the overflow chevron)
    /// instead of the left (next to the app menu).
    pub theme_button_right: bool,
}

impl Default for HostSettings {
    fn default() -> Self {
        Self {
            icons_in_tabs: true,
            theme_button_right: false,
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
    /// The window's real icon, ready to render (present only when extracted).
    pub icon: Option<Arc<RenderImage>>,
}

/// A UI-facing snapshot of the active host group.
#[derive(Clone, Default)]
pub struct HostInfo {
    pub group: Option<GroupId>,
    pub tabs: Vec<TabInfo>,
    pub active: Option<TabId>,
    pub icons_in_tabs: bool,
    /// Theme toggle on the right of the title bar (after the overflow chevron).
    pub theme_button_right: bool,
    /// True while a UI overlay (menu) is open and the member is hidden.
    pub overlay: bool,
    /// True while the tab-overflow dropdown is open and the member is hidden.
    pub tab_menu_open: bool,
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
                            title: t.window.title.clone(),
                            icon: self.icons.get(&t.window.hwnd).cloned().flatten(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            active: group.and_then(|g| g.active),
            icons_in_tabs: self.settings.icons_in_tabs,
            theme_button_right: self.settings.theme_button_right,
            overlay: self.overlay,
            tab_menu_open: self.tab_menu_open,
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
            Cmd::ReorderTab { group, from, to } => self.reorder(group, from, to),
            Cmd::BeginPick => self.begin_pick(cx),
            Cmd::ToggleIconsInTabs => self.toggle_icons_in_tabs(),
            Cmd::SetOverlay(open) => self.overlay = open,
            Cmd::ToggleOverlay => self.overlay = !self.overlay,
            Cmd::SetTabMenu(open) => self.tab_menu_open = open,
            Cmd::ToggleTabMenu => self.tab_menu_open = !self.tab_menu_open,
            Cmd::SetThemeButtonRight(right) => self.settings.theme_button_right = right,
            Cmd::Shutdown => self.shutdown(),
        }
        self.sync_worker();
        cx.notify();
    }

    /// Restore every hosted window to normal, then stop the worker. Safe to
    /// call more than once; never leaves a member hidden (spec 02/06).
    fn shutdown(&mut self) {
        self.end_pick();
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
        self.groups.remove_tab(group, tab);
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

    /// A destroyed foreign window is no longer hostable: drop its tab.
    fn window_gone(&mut self, hwnd: isize) {
        self.styling.remove(&hwnd);
        self.icons.remove(&hwnd);
        self.windows.retain(|w| w.hwnd != hwnd);
        self.groups.window_destroyed(hwnd);
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
                WinEvent::TitleChanged(hwnd) if self.styling.contains_key(&hwnd) => {
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
        // While a UI overlay (menu) or the tab-overflow dropdown is open, hide
        // the active member too: an owned window always paints above the host,
        // so it would cover them otherwise.
        let active = if self.overlay || self.tab_menu_open {
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
