use std::collections::HashSet;

use super::Controller;
use crate::config;
use crate::win::{enumerate_windows, is_blacklisted};

impl Controller {
    pub(super) fn set_saved_menu(&mut self, open: bool) {
        if open {
            self.close_other_dropdowns();
            self.windows = enumerate_windows();
            self.refresh_saved_avail();
        }
        self.saved_menu_open = open;
    }

    pub(super) fn refresh_saved_avail(&mut self) {
        self.saved_avail = self
            .saved_sets
            .iter()
            .map(|set| (set.id, self.resolve_set(set).len()))
            .collect();
    }

    pub(super) fn resolve_set(&self, set: &config::SavedSet) -> Vec<isize> {
        let mut used: HashSet<isize> = self.styling.keys().copied().collect();
        let mut chosen = Vec::new();
        for tab in &set.tabs {
            let pick = self
                .windows
                .iter()
                .filter(|w| {
                    !used.contains(&w.hwnd)
                        && !is_blacklisted(&w.class)
                        && !w.elevated
                        && w.exe_path.eq_ignore_ascii_case(&tab.exe_path)
                })
                .min_by_key(|w| u8::from(w.title != tab.title_hint))
                .map(|w| w.hwnd);
            if let Some(hwnd) = pick {
                used.insert(hwnd);
                chosen.push(hwnd);
            }
        }
        chosen
    }

    pub(super) fn commit_save_set(&mut self, name: String) {
        self.saved_prompt = None;
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(group) = self.active_group.and_then(|id| self.groups.group(id)) else {
            return;
        };
        let tabs: Vec<config::SavedTab> = group
            .tabs
            .iter()
            .map(|t| config::SavedTab {
                exe_path: t.window.exe_path.clone(),
                exe_name: t.window.exe_name.clone(),
                title_hint: t.window.title.clone(),
                accent: self.tab_colors.get(&t.window.hwnd).copied(),
            })
            .collect();
        if tabs.is_empty() {
            return;
        }
        if let Some(existing) = self.saved_sets.iter_mut().find(|s| s.name == name) {
            existing.tabs = tabs;
        } else {
            let id = self.saved_sets.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            self.saved_sets.push(config::SavedSet { id, name, tabs });
        }
        self.persist_sets();
        self.refresh_saved_avail();
    }

    #[allow(dead_code)]
    pub(super) fn update_set(&mut self, id: u64) {
        let Some(group) = self.active_group.and_then(|gid| self.groups.group(gid)) else {
            return;
        };
        let tabs: Vec<config::SavedTab> = group
            .tabs
            .iter()
            .map(|t| config::SavedTab {
                exe_path: t.window.exe_path.clone(),
                exe_name: t.window.exe_name.clone(),
                title_hint: t.window.title.clone(),
                accent: self.tab_colors.get(&t.window.hwnd).copied(),
            })
            .collect();
        if tabs.is_empty() {
            return;
        }
        if let Some(set) = self.saved_sets.iter_mut().find(|s| s.id == id) {
            set.tabs = tabs;
            self.persist_sets();
            self.refresh_saved_avail();
        }
    }

    pub(super) fn commit_rename_set(&mut self, id: u64, name: String) {
        self.saved_prompt = None;
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        if let Some(set) = self.saved_sets.iter_mut().find(|s| s.id == id) {
            set.name = name;
            self.persist_sets();
        }
    }

    pub(super) fn delete_set(&mut self, id: u64) {
        let before = self.saved_sets.len();
        self.saved_sets.retain(|s| s.id != id);
        if self.saved_sets.len() != before {
            self.persist_sets();
        }
    }

    pub(super) fn restore_set(&mut self, id: u64) {
        let Some(set) = self.saved_sets.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        self.windows = enumerate_windows();
        let hwnds = self.resolve_set(&set);
        for hwnd in &hwnds {
            self.add_window(*hwnd);
        }
        for (i, tab) in set.tabs.iter().enumerate() {
            if let Some(accent) = tab.accent {
                if let Some(hwnd) = hwnds.get(i) {
                    if let Some((group, tab_id)) = self.tab_of_hwnd(*hwnd) {
                        self.set_tab_color(group, tab_id, Some(accent));
                        continue;
                    }
                }
                if let Some(gid) = self.active_group {
                    if let Some(g) = self.groups.group(gid) {
                        if let Some(t) = g.tabs.get(i) {
                            if t.window.exe_path.eq_ignore_ascii_case(&tab.exe_path) {
                                self.set_tab_color(gid, t.id, Some(accent));
                            }
                        }
                    }
                }
            }
        }
        self.saved_menu_open = false;
    }

    fn persist_sets(&self) {
        let sets = config::SavedSets {
            schema_version: config::SCHEMA_VERSION,
            sets: self.saved_sets.clone(),
        };
        let _ = config::save_sets(&sets);
    }
}
