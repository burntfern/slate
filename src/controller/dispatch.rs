use gpui::Context;

use super::dto::{Cmd, SavedPrompt, TabMenu};
use super::Controller;
use crate::config;
use crate::win::{activate_window, enumerate_windows, install_window_icon};

impl Controller {
    pub fn dispatch(&mut self, cmd: Cmd, cx: &mut Context<Self>) {
        self.invalidate_info();
        self.drain_events();
        match cmd {
            Cmd::Refresh => self.windows = enumerate_windows(),
            Cmd::HostReady { hwnd, titlebar } => {
                self.host_hwnd = Some(hwnd);
                self.titlebar = titlebar;
                install_window_icon(hwnd);
                if let Some(tray) = &self.tray {
                    tray.set_host(hwnd);
                }
                self.worker.start();
                self.worker.install_move_tracker(hwnd);
            }
            Cmd::SelectTab { group, tab } => self.select_tab(group, tab),
            Cmd::Ungroup { group, tab } => {
                if let Some(hwnd) = self.hwnd_of_tab(group, tab) {
                    self.ungroup(group, tab);
                    activate_window(hwnd);
                }
            }
            Cmd::CloseTab { group, tab } => self.close_tab(group, tab),
            Cmd::DetachTabOut {
                from,
                screen_x,
                screen_y,
            } => self.detach_tab_out(from, screen_x, screen_y),
            Cmd::ReorderTab { group, from, to } => self.reorder(group, from, to),
            Cmd::BeginPick => self.begin_pick(cx),
            Cmd::ToggleIconsInTabs => self.toggle_icons_in_tabs(),
            Cmd::SetOverlay(open) => self.overlay = open,
            Cmd::ToggleOverlay => self.overlay = !self.overlay,
            Cmd::SetTabMenu(open) => {
                if open {
                    self.close_other_dropdowns();
                }
                self.tab_menu_open = open;
            }
            Cmd::ToggleTabMenu => {
                let open = !self.tab_menu_open;
                if open {
                    self.close_other_dropdowns();
                }
                self.tab_menu_open = open;
            }
            Cmd::AddWindow { hwnd } => self.add_window(hwnd),
            Cmd::CloseOtherTabs { group, tab } => self.close_other_tabs(group, tab),
            Cmd::CloseTabsToRight { group, tab } => self.close_tabs_to_right(group, tab),
            Cmd::CloseTabsToLeft { group, tab } => self.close_tabs_to_left(group, tab),
            Cmd::OpenTabMenu { tab, at } => {
                self.tab_menu = Some(TabMenu { tab, at });
                self.tab_menu_open = false;
                self.picker_open = false;
                self.saved_menu_open = false;
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
            Cmd::SetSavedMenu(open) => self.set_saved_menu(open),
            Cmd::ToggleSavedMenu => {
                let open = !self.saved_menu_open;
                self.set_saved_menu(open);
            }
            Cmd::BeginSaveSet => {
                self.saved_menu_open = false;
                self.saved_prompt = Some(SavedPrompt::Save);
            }
            Cmd::BeginRenameSet { id } => {
                if self.saved_sets.iter().any(|s| s.id == id) {
                    self.saved_menu_open = false;
                    self.saved_prompt = Some(SavedPrompt::Rename(id));
                }
            }
            Cmd::CancelSavedPrompt => self.saved_prompt = None,
            Cmd::CommitSaveSet { name } => self.commit_save_set(name),
            Cmd::CommitRenameSet { id, name } => self.commit_rename_set(id, name),
            #[allow(deprecated)]
            Cmd::UpdateSet { id } => self.update_set(id),
            Cmd::RestoreSet { id } => self.restore_set(id),
            Cmd::DeleteSet { id } => self.delete_set(id),
            Cmd::ToggleTabBadge => self.toggle_tab_badge(),
            Cmd::ToggleHoverPreview => self.toggle_hover_preview(),
            Cmd::ToggleTheme => self.toggle_theme(cx),
            Cmd::SetTabColor { group, tab, color } => self.set_tab_color(group, tab, color),
            Cmd::PreviewTab { tab, anchor } => self.preview_hover(tab, anchor, cx),
            Cmd::PreviewHide { tab } => self.preview_unhover(tab, cx),
            Cmd::PreviewCardHover(hovered) => self.preview_card_hover(hovered, cx),
            Cmd::Shutdown => self.shutdown(),
            Cmd::ToggleTabBar => {
                self.settings.tabs.show_bar = !self.settings.tabs.show_bar;
                self.save_settings();
            }
            Cmd::OpenRepository => {
                crate::win::shell::open_url(crate::core::update::REPOSITORY_URL);
            }
            Cmd::CheckForUpdates => self.check_for_updates(cx),
            Cmd::OpenSlateFolder => {
                crate::win::shell::open_folder(&config::paths::data_dir());
            }
        }
        self.sync_worker();
        cx.notify();
    }

    pub(super) fn close_other_dropdowns(&mut self) {
        self.picker_open = false;
        self.saved_menu_open = false;
        self.tab_menu_open = false;
        self.tab_menu = None;
    }
}
