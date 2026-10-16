//! Host-window controller. Commands flow `UI → Cmd → controller → Win32 → core`:
//! the controller enumerates windows, registers members with the hosting engine,
//! keeps `GroupSet` state, and publishes the host layout to the reposition worker.

mod dispatch;
mod dto;
mod events;
mod hover;
mod pick;
mod picker;
mod sets;
mod settings;
mod tabs;

pub use dto::{
    Cmd, HostInfo, HoverPreview, PickerRow, SavedPrompt, SavedSetInfo, TabInfo, TabMenu,
};

use ely_gpui_component::feedback::Toaster;
use gpui::{AppContext, Context, Entity, RenderImage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use self::dto::app_name;
use crate::config;
use crate::core::{GroupId, GroupSet, TabId, WindowIdentity};
use crate::win::{
    activate_window, enumerate_windows, unregister_member, Highlight, HostWorker, Janitor,
    KeyboardCapture, ShortcutCapture, StylingState, WinEventPump,
};

/// Hover-card open/close delay, matching ely's `HoverCard`.
pub(super) const PREVIEW_OPEN_MS: u64 = 500;
pub(super) const PREVIEW_LINGER_MS: u64 = 200;

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
    /// Ring drawn over the window the pointer is on while picking or dragging.
    highlight: Option<Highlight>,
    pub settings: config::Settings,
    icons: HashMap<isize, Option<Arc<RenderImage>>>,
    overlay: bool,
    tab_menu_open: bool,
    picker_open: bool,
    picker_rows: Vec<PickerRow>,
    saved_sets: Vec<config::SavedSet>,
    saved_menu_open: bool,
    saved_prompt: Option<SavedPrompt>,
    tab_menu: Option<TabMenu>,
    renaming: Option<(GroupId, TabId)>,
    /// Display-title overrides for renamed tabs, keyed by hwnd. Takes precedence
    /// over the live window title and survives retitle events.
    rename_overrides: HashMap<isize, String>,
    /// Per-tab accent colors (chart-hue index), keyed by hwnd. Session-only.
    tab_colors: HashMap<isize, usize>,
    hover_preview: Option<HoverPreview>,
    hover_tab: Option<TabId>,
    card_hovered: bool,
    /// Bumped on every hover change to cancel stale open/linger timers.
    preview_gen: u64,
    /// Last captured frame per window, so a hidden tab still shows something.
    snapshots: HashMap<isize, Arc<RenderImage>>,
    shortcuts: Option<ShortcutCapture>,
    tray: Option<crate::win::TrayIcon>,
    /// The foreign window currently being dragged, watched for the Ctrl-drag
    /// "add as tab" gesture.
    drag: Option<isize>,
    drag_active: bool,
    _pump: Option<WinEventPump>,
}

impl Controller {
    pub fn new(cx: &mut Context<Self>) -> Self {
        config::ensure();
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
            settings: config::load(),
            icons: HashMap::new(),
            overlay: false,
            tab_menu_open: false,
            picker_open: false,
            picker_rows: Vec::new(),
            saved_sets: config::load_sets().sets,
            saved_menu_open: false,
            saved_prompt: None,
            tab_menu: None,
            renaming: None,
            rename_overrides: HashMap::new(),
            tab_colors: HashMap::new(),
            hover_preview: None,
            hover_tab: None,
            card_hovered: false,
            preview_gen: 0,
            snapshots: HashMap::new(),
            shortcuts: ShortcutCapture::install(),
            tray: crate::win::TrayIcon::start(),
            drag: None,
            drag_active: false,
            _pump: WinEventPump::install(),
            toaster,
        };
        Self::start_event_pump(cx);
        controller.apply_theme(cx);
        controller
    }

    /// Drain OS window events that arrive while the user does nothing inside
    /// Slate, so the strip stays live between commands.
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

    fn tick(&mut self, cx: &mut Context<Self>) {
        let mut changed = self.drain_events();
        let shortcuts = self
            .shortcuts
            .as_ref()
            .map(|capture| capture.drain())
            .unwrap_or_default();
        for shortcut in shortcuts {
            changed |= self.apply_shortcut(shortcut);
        }
        self.pump_drag();
        if changed {
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
                            accent: self.tab_colors.get(&t.window.hwnd).copied(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            active: group.and_then(|g| g.active),
            icons_in_tabs: self.settings.tabs.icons,
            show_tab_badge: self.settings.tabs.count_badge,
            hover_preview_enabled: self.settings.tabs.hover_preview,
            overlay: self.overlay,
            tab_menu_open: self.tab_menu_open,
            picker_open: self.picker_open,
            picker_rows: self.picker_rows.clone(),
            saved_menu_open: self.saved_menu_open,
            saved_sets: self
                .saved_sets
                .iter()
                .map(|s| SavedSetInfo {
                    name: s.name.clone(),
                    count: s.tabs.len(),
                    available: self.set_available(s),
                })
                .collect(),
            saved_prompt: self.saved_prompt,
            tab_menu: self.tab_menu.clone(),
            renaming: self.renaming.map(|(_, tab)| tab),
            picking: self.picking,
            hover_preview: self.hover_preview.clone(),
            show_tab_bar: self.settings.tabs.show_bar,
        }
    }

    /// The toaster the host view renders the pick hint through. Shared so the
    /// view can observe arrivals/expiries and re-render.
    pub fn toaster(&self) -> Entity<Toaster> {
        self.toaster.clone()
    }

    /// Restore every hosted window to normal, then stop the worker. Safe to call
    /// more than once; never leaves a member hidden.
    fn shutdown(&mut self) {
        self.end_pick();
        self.hide_hover_preview();
        // Remember the member that was on screen so it can be re-focused once it
        // is restored; a restored window is missing from the taskbar and unfocused
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
        self.drag = None;
        self.drag_active = false;
        if let Some(highlight) = self.highlight.as_ref() {
            highlight.hide();
        }
        self.worker.stop();
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
