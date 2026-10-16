use super::Controller;
use crate::core::{GroupId, TabId};
use crate::win::{
    activate_window, close_window, is_blacklisted, move_window, register_member, unregister_member,
    Shortcut,
};

impl Controller {
    /// Register a running window as a tab in the active group.
    pub(super) fn add_window(&mut self, hwnd: isize) {
        let Some(host) = self.host_hwnd else {
            return;
        };
        let Some(identity) = self.windows.iter().find(|w| w.hwnd == hwnd).cloned() else {
            return;
        };
        if is_blacklisted(&identity.class) || identity.elevated {
            return;
        }
        if self.groups.contains_window(hwnd) {
            return;
        }
        // Register the window first; only touch the group state once it is really
        // ours, so a failed registration never leaves an empty active group.
        let Some(state) = register_member(host, hwnd) else {
            return;
        };
        self.styling.insert(hwnd, state);
        let group = match self.active_group {
            Some(id) => id,
            None => {
                let id = self.groups.create_group(Vec::new());
                self.active_group = Some(id);
                id
            }
        };
        if self.settings.tabs.icons {
            self.ensure_icon(hwnd);
        }
        self.groups.add_window(group, identity);
        if let Some((_, tab)) = self.tab_of_hwnd(hwnd) {
            self.groups.set_active(group, tab);
            self.activate_member(hwnd);
        }
    }

    /// Show the selected tab's member; siblings stay hidden.
    pub(super) fn select_tab(&mut self, group: GroupId, tab: TabId) {
        self.groups.set_active(group, tab);
        if let Some(hwnd) = self.hwnd_of_tab(group, tab) {
            self.activate_member(hwnd);
        }
    }

    /// Apply a global shortcut to the active group. Returns true when it changed
    /// anything (so the caller re-renders / re-syncs the worker).
    pub(super) fn apply_shortcut(&mut self, shortcut: Shortcut) -> bool {
        let Some(group) = self.active_group else {
            return false;
        };
        let Some((active, ids)) = self.groups.group(group).map(|g| {
            (
                g.active,
                g.tabs.iter().map(|t| t.id).collect::<Vec<TabId>>(),
            )
        }) else {
            return false;
        };
        let len = ids.len();
        if len == 0 {
            return false;
        }
        let current = active
            .and_then(|id| ids.iter().position(|t| *t == id))
            .unwrap_or(0);
        match shortcut {
            Shortcut::NextTab => {
                self.select_tab(group, ids[(current + 1) % len]);
                true
            }
            Shortcut::PrevTab => {
                self.select_tab(group, ids[(current + len - 1) % len]);
                true
            }
            Shortcut::CloseTab => {
                self.ungroup(group, ids[current]);
                true
            }
            Shortcut::SelectIndex(n) => {
                let Some(ix) = (n as usize).checked_sub(1) else {
                    return false;
                };
                if ix < len {
                    self.select_tab(group, ids[ix]);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Remove a tab and restore its window to normal. Never closes the app.
    pub(super) fn ungroup(&mut self, group: GroupId, tab: TabId) {
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        if let Some(state) = self.styling.remove(&hwnd) {
            unregister_member(hwnd, &state);
        }
        self.icons.remove(&hwnd);
        self.snapshots.remove(&hwnd);
        self.rename_overrides.remove(&hwnd);
        self.tab_colors.remove(&hwnd);
        self.groups.remove_tab(group, tab);
    }

    /// Detach the tab at `from` in the active group and drop its window at the
    /// given screen position (the drag-out gesture).
    pub(super) fn detach_tab_out(&mut self, from: usize, screen_x: f32, screen_y: f32) {
        let Some(group) = self.active_group else {
            return;
        };
        let Some(tab) = self
            .groups
            .group(group)
            .and_then(|g| g.tabs.get(from))
            .map(|t| t.id)
        else {
            return;
        };
        self.detach_tab_at(group, tab, screen_x, screen_y);
    }

    /// Ungroup `tab` and move its restored window so the cursor lands near its
    /// top edge, keeping the size it had before it was hosted.
    fn detach_tab_at(&mut self, group: GroupId, tab: TabId, screen_x: f32, screen_y: f32) {
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        let size = self
            .styling
            .get(&hwnd)
            .map(|s| (s.rect.2 - s.rect.0, s.rect.3 - s.rect.1))
            .filter(|(w, h)| *w > 0 && *h > 0);
        self.ungroup(group, tab);
        let (w, h) = size.unwrap_or((900, 640));
        let x = screen_x as i32 - w / 2;
        let y = (screen_y as i32 - 24).max(0);
        move_window(hwnd, x, y, w, h);
        activate_window(hwnd);
    }

    /// Set (or clear) a tab's accent color. Session-only, keyed by hwnd.
    pub(super) fn set_tab_color(&mut self, group: GroupId, tab: TabId, color: Option<usize>) {
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        match color {
            Some(index) => {
                self.tab_colors.insert(hwnd, index);
            }
            None => {
                self.tab_colors.remove(&hwnd);
            }
        }
    }

    /// Post `WM_CLOSE` and let the app quit on its own terms. The member is
    /// released first so the host stops pinning it; the destroy event then clears
    /// the tab via `window_gone`.
    pub(super) fn close_tab(&mut self, group: GroupId, tab: TabId) {
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        if let Some(state) = self.styling.remove(&hwnd) {
            unregister_member(hwnd, &state);
        }
        close_window(hwnd);
    }

    /// Reorder a tab within its group by index.
    pub(super) fn reorder(&mut self, group: GroupId, from: usize, to: usize) {
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
    pub(super) fn close_other_tabs(&mut self, group: GroupId, keep: TabId) {
        let Some(g) = self.groups.group(group) else {
            return;
        };
        let ids: Vec<TabId> = g
            .tabs
            .iter()
            .map(|t| t.id)
            .filter(|id| *id != keep)
            .collect();
        for id in ids {
            self.ungroup(group, id);
        }
    }

    /// Ungroup every tab positioned to the right of `tab`.
    pub(super) fn close_tabs_to_right(&mut self, group: GroupId, tab: TabId) {
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

    /// Ungroup every tab positioned to the left of `tab`.
    pub(super) fn close_tabs_to_left(&mut self, group: GroupId, tab: TabId) {
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
    pub(super) fn rename_tab(&mut self, group: GroupId, tab: TabId, title: String) {
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
}
