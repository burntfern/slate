mod dispatch;
mod dto;
mod events;
mod hover;
mod pick;
mod picker;
mod sets;
mod settings;
mod tabs;
mod update;

pub use dto::{
    Cmd, HostInfo, HoverPreview, PickerRow, SavedPrompt, SavedSetInfo, TabInfo, TabMenu,
};

use ely_gpui_component::feedback::Toaster;
use gpui::{AppContext, Context, Entity, RenderImage, SharedString};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use self::dto::app_name;
use crate::config;
use crate::core::{GroupId, GroupSet, TabId, UpdateState, WindowIdentity};
use crate::win::{
    activate_process, activate_window, enumerate_windows, unregister_member, Highlight, HostWorker,
    Janitor, KeyboardCapture, ShortcutCapture, StylingState, WinEventPump,
};
use std::os::windows::process::CommandExt;

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
    highlight: Option<Highlight>,
    pub settings: config::Settings,
    icons: HashMap<isize, Option<Arc<RenderImage>>>,
    overlay: bool,
    tab_menu_open: bool,
    picker_open: bool,
    picker_rows: Vec<PickerRow>,
    saved_sets: Vec<config::SavedSet>,
    saved_avail: HashMap<u64, usize>,
    saved_menu_open: bool,
    saved_prompt: Option<SavedPrompt>,
    tab_menu: Option<TabMenu>,
    renaming: Option<(GroupId, TabId)>,
    rename_overrides: HashMap<isize, String>,
    tab_colors: HashMap<isize, usize>,
    hover_preview: Option<HoverPreview>,
    hover_tab: Option<TabId>,
    card_hovered: bool,
    preview_gen: u64,
    snapshots: HashMap<isize, Arc<RenderImage>>,
    shortcuts: Option<ShortcutCapture>,
    tray: Option<crate::win::TrayIcon>,
    drag: Option<isize>,
    drag_active: bool,
    update: UpdateState,
    info_cache: RefCell<Option<Arc<HostInfo>>>,
    index_cache: RefCell<Option<Index>>,
    _pump: Option<WinEventPump>,
}

#[derive(Default)]
struct Index {
    generation: u64,
    by_hwnd: HashMap<isize, (GroupId, TabId)>,
    by_tab: HashMap<(GroupId, TabId), isize>,
}



impl Controller {
    pub fn new(cx: &mut Context<Self>) -> Self {
        config::ensure();
        Self::execute_startup_plan(update::startup_install());
        let toaster = cx.new(|_| Toaster::default());
        let settings = config::load();
        let update = UpdateState::from_cached(
            crate::core::update::CURRENT_VERSION,
            settings.updates.latest.as_deref(),
        );
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
            settings,
            icons: HashMap::new(),
            overlay: false,
            tab_menu_open: false,
            picker_open: false,
            picker_rows: Vec::new(),
            saved_sets: config::load_sets().sets,
            saved_avail: HashMap::new(),
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
            update,
            info_cache: RefCell::new(None),
            index_cache: RefCell::new(None),
            _pump: WinEventPump::install(),
            toaster,
        };
        Self::start_event_pump(cx);
        controller.apply_theme(cx);
        controller
    }

    fn execute_startup_plan(plan: crate::core::install::Plan) {
        match plan {
            crate::core::install::Plan::Dev => {}
            crate::core::install::Plan::Run { exe, canonical } => {
                config::save_install(&config::InstallRecord {
                    path: exe,
                    version: crate::core::update::CURRENT_VERSION.to_string(),
                    pid: std::process::id(),
                });
                if canonical {
                    let _ = std::fs::remove_file(crate::core::install::backup_path(
                        &config::canonical_exe_path(),
                    ));
                }
            }
            crate::core::install::Plan::FocusRunning { pid } => {
                if activate_process(pid) {
                    crate::win::shell::info_message(
                        "Slate is already running",
                        "Switching to the open window.",
                    );
                }
                std::process::exit(0);
            }
            crate::core::install::Plan::Bootstrap { source } => {
                if let Err(err) = crate::win::update::bootstrap(&source) {
                    crate::win::shell::info_message("First-run bootstrap failed", &err);
                    std::process::exit(1);
                }
                let _ = std::process::Command::new(config::canonical_exe_path())
                    .creation_flags(0)
                    .spawn();
                std::process::exit(0);
            }
            crate::core::install::Plan::Update { candidate } => {
                if let Err(err) = crate::win::update::refresh_install_from(&candidate) {
                    crate::win::shell::info_message("Update from downloaded release failed", &err);
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            crate::core::install::Plan::Defer { canonical } => {
                let _ = std::process::Command::new(&canonical)
                    .creation_flags(0)
                    .spawn();
                std::process::exit(0);
            }
        }
    }

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
            self.invalidate_info();
            self.sync_worker();
            cx.notify();
        }
    }

    pub fn host_info(&self) -> Arc<HostInfo> {
        if let Some(info) = self.info_cache.borrow().clone() {
            return info;
        }
        let info = Arc::new(self.build_host_info());
        self.info_cache.replace(Some(info.clone()));
        info
    }

    fn invalidate_info(&mut self) {
        self.info_cache.replace(None);
    }

    fn build_host_info(&self) -> HostInfo {
        let group = self.active_group.and_then(|id| self.groups.group(id));
        let caption = match group.map(|g| g.tabs.len()).unwrap_or(0) {
            0 => SharedString::from("No windows yet"),
            1 => SharedString::from("1 window open"),
            n => SharedString::from(format!("{n} windows open")),
        };
        let version_label =
            SharedString::from(format!("Slate v{}", crate::core::update::CURRENT_VERSION));
        let update_label = match &self.update {
            UpdateState::Checking => SharedString::from("Checking for updates\u{2026}"),
            UpdateState::Available(version) => {
                SharedString::from(format!("Update available: v{version}"))
            }
            UpdateState::UpToDate => SharedString::from("Slate is up to date"),
            UpdateState::Idle => SharedString::from("Check for updates"),
            UpdateState::Failed => SharedString::from("Update check failed - retry"),
        };
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
                .map(|s| {
                    let available = self.saved_avail.get(&s.id).copied().unwrap_or(0);
                    SavedSetInfo {
                        id: s.id,
                        name: s.name.clone(),
                        available,
                        total: s.tabs.len(),
                    }
                })
                .collect(),
            saved_prompt: self.saved_prompt,
            tab_menu: self.tab_menu.clone(),
            renaming: self.renaming.map(|(_, tab)| tab),
            picking: self.picking,
            hover_preview: self.hover_preview.clone(),
            show_tab_bar: self.settings.tabs.show_bar,
            update: self.update.clone(),
            caption,
            version_label,
            update_label,
        }
    }

    // refresh_saved_avail defined in sets.rs to avoid duplication

    pub fn toaster(&self) -> Entity<Toaster> {
        self.toaster.clone()
    }

    fn shutdown(&mut self) {
        self.end_pick();
        self.hide_hover_preview();
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

    fn reconcile_active_group(&mut self) {
        self.groups.remove_empty();
        let valid = self
            .active_group
            .is_some_and(|id| self.groups.group(id).is_some());
        if !valid {
            self.active_group = self.groups.groups().first().map(|g| g.id);
        }
    }

    fn active_hwnd(&self, group: GroupId) -> Option<isize> {
        let g = self.groups.group(group)?;
        let tab = g.active?;
        self.hwnd_of_tab(group, tab)
    }

    fn ensure_index(&self) {
        let generation = self.groups.generation();
        if self
            .index_cache
            .borrow()
            .as_ref()
            .is_some_and(|index| index.generation == generation)
        {
            return;
        }
        let mut index = Index {
            generation,
            ..Default::default()
        };
        for group in self.groups.groups() {
            for tab in &group.tabs {
                index.by_hwnd.insert(tab.window.hwnd, (group.id, tab.id));
                index.by_tab.insert((group.id, tab.id), tab.window.hwnd);
            }
        }
        *self.index_cache.borrow_mut() = Some(index);
    }

    fn hwnd_of_tab(&self, group: GroupId, tab: TabId) -> Option<isize> {
        self.ensure_index();
        self.index_cache
            .borrow()
            .as_ref()
            .and_then(|index| index.by_tab.get(&(group, tab)).copied())
    }

    fn tab_of_hwnd(&self, hwnd: isize) -> Option<(GroupId, TabId)> {
        self.ensure_index();
        self.index_cache
            .borrow()
            .as_ref()
            .and_then(|index| index.by_hwnd.get(&hwnd).copied())
    }
}
