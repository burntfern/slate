use std::collections::HashSet;

use super::Controller;
use crate::config;
use crate::win::{enumerate_windows, is_blacklisted};

impl Controller {
    /// Open or close the Saved-tab-sets panel. Opening refreshes the window list
    /// so the per-set availability is current.
    pub(super) fn set_saved_menu(&mut self, open: bool) {
        self.saved_menu_open = open;
        if open {
            self.close_other_dropdowns();
            self.windows = enumerate_windows();
        }
    }

    /// How many of a set's saved tabs currently match a running, eligible window
    /// that isn't already hosted.
    pub(super) fn set_available(&self, set: &config::SavedSet) -> usize {
        let mut used: HashSet<isize> = self.styling.keys().copied().collect();
        let mut count = 0;
        for tab in &set.tabs {
            if let Some(hwnd) = self
                .windows
                .iter()
                .find(|w| {
                    !used.contains(&w.hwnd)
                        && !is_blacklisted(&w.class)
                        && !w.elevated
                        && w.exe_path.eq_ignore_ascii_case(&tab.exe_path)
                })
                .map(|w| w.hwnd)
            {
                used.insert(hwnd);
                count += 1;
            }
        }
        count
    }

    /// Snapshot the active group's tabs as a named set and persist it. An
    /// existing set with the same name is replaced so the list stays tidy.
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
            })
            .collect();
        if tabs.is_empty() {
            return;
        }
        if let Some(existing) = self.saved_sets.iter_mut().find(|s| s.name == name) {
            existing.tabs = tabs;
        } else {
            self.saved_sets.push(config::SavedSet { name, tabs });
        }
        self.persist_sets();
    }

    /// Rename the set at `index` and persist.
    pub(super) fn commit_rename_set(&mut self, index: usize, name: String) {
        self.saved_prompt = None;
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        if let Some(set) = self.saved_sets.get_mut(index) {
            set.name = name;
            self.persist_sets();
        }
    }

    /// Delete the set at `index` and persist.
    pub(super) fn delete_set(&mut self, index: usize) {
        if index < self.saved_sets.len() {
            self.saved_sets.remove(index);
            self.persist_sets();
        }
    }

    /// Add every running window that matches a saved tab back as a tab. Matching
    /// is by executable path (case-insensitive), preferring a title match;
    /// already-hosted windows are skipped and nothing is ever launched.
    pub(super) fn restore_set(&mut self, index: usize) {
        let Some(set) = self.saved_sets.get(index).cloned() else {
            return;
        };
        self.windows = enumerate_windows();
        let mut used: HashSet<isize> = self.styling.keys().copied().collect();
        let mut chosen: Vec<isize> = Vec::new();
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
        for hwnd in chosen {
            self.add_window(hwnd);
        }
        self.saved_menu_open = false;
    }

    /// Persist the current saved-set list.
    fn persist_sets(&self) {
        let sets = config::SavedSets {
            schema_version: config::SCHEMA_VERSION,
            sets: self.saved_sets.clone(),
        };
        config::save_sets(&sets);
    }
}
