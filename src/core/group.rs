use super::identity::{GroupId, TabId, WindowIdentity};

#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub window: WindowIdentity,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub collapsed: bool,
    pub tabs: Vec<Tab>,
    pub active: Option<TabId>,
}

/// Outcome of a mutation. `Applied` means state changed; `Noop` means the
/// operation was idempotent (e.g. removing a window that is already gone).
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Applied,
    Noop,
}

/// All grouping state. Pure data — no OS calls, fully unit-testable.
#[derive(Debug, Default)]
pub struct GroupSet {
    groups: Vec<Group>,
    next_group: u64,
    next_tab: u64,
}

impl GroupSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn group_mut(&mut self, id: GroupId) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// True if this window already belongs to any group.
    pub fn contains_window(&self, hwnd: isize) -> bool {
        self.groups
            .iter()
            .any(|g| g.tabs.iter().any(|t| t.window.hwnd == hwnd))
    }

    /// Create a group, optionally seeding it with windows. The first window
    /// becomes active.
    pub fn create_group(&mut self, name: &str, windows: Vec<WindowIdentity>) -> GroupId {
        let id = GroupId(self.next_group);
        self.next_group += 1;
        let mut tabs = Vec::new();
        for w in windows {
            let tid = TabId(self.next_tab);
            self.next_tab += 1;
            tabs.push(Tab { id: tid, window: w });
        }
        let active = tabs.first().map(|t| t.id);
        self.groups.push(Group {
            id,
            name: name.to_string(),
            collapsed: false,
            tabs,
            active,
        });
        id
    }

    /// Dissolving never touches the windows themselves — they stay open.
    pub fn dissolve(&mut self, id: GroupId) -> Effect {
        let before = self.groups.len();
        self.groups.retain(|g| g.id != id);
        if self.groups.len() == before {
            Effect::Noop
        } else {
            Effect::Applied
        }
    }

    /// Add a window to a group. Idempotent: adding a window that is already
    /// grouped anywhere is a no-op.
    pub fn add_window(&mut self, id: GroupId, window: WindowIdentity) -> Effect {
        if self.contains_window(window.hwnd) {
            return Effect::Noop;
        }
        let tid = TabId(self.next_tab);
        self.next_tab += 1;
        let Some(g) = self.group_mut(id) else {
            return Effect::Noop;
        };
        if g.active.is_none() {
            g.active = Some(tid);
        }
        g.tabs.push(Tab { id: tid, window });
        Effect::Applied
    }

    /// Remove a tab (detach or close — caller decides what to do with the
    /// window). Removing an unknown tab is a no-op so stale destroy events
    /// from the OS are harmless.
    pub fn remove_tab(&mut self, group: GroupId, tab: TabId) -> Effect {
        let Some(g) = self.group_mut(group) else {
            return Effect::Noop;
        };
        let before = g.tabs.len();
        g.tabs.retain(|t| t.id != tab);
        if g.tabs.len() == before {
            return Effect::Noop;
        }
        if g.active == Some(tab) {
            g.active = g.tabs.first().map(|t| t.id);
        }
        Effect::Applied
    }

    /// Drop a window from whichever group holds it (used for destroy events).
    pub fn window_destroyed(&mut self, hwnd: isize) -> Effect {
        let mut applied = false;
        for g in self.groups.iter_mut() {
            let before = g.tabs.len();
            let removed_active = g
                .tabs
                .iter()
                .find(|t| t.window.hwnd == hwnd)
                .map(|t| t.id == g.active.unwrap_or(TabId(u64::MAX)))
                .unwrap_or(false);
            g.tabs.retain(|t| t.window.hwnd != hwnd);
            if g.tabs.len() != before {
                applied = true;
                if removed_active {
                    g.active = g.tabs.first().map(|t| t.id);
                }
            }
        }
        if applied {
            Effect::Applied
        } else {
            Effect::Noop
        }
    }

    /// Reorder a tab to a new index within its group.
    pub fn reorder(&mut self, group: GroupId, tab: TabId, to_index: usize) -> Effect {
        let Some(g) = self.group_mut(group) else {
            return Effect::Noop;
        };
        let Some(from) = g.tabs.iter().position(|t| t.id == tab) else {
            return Effect::Noop;
        };
        let t = g.tabs.remove(from);
        let to = to_index.min(g.tabs.len());
        g.tabs.insert(to, t);
        Effect::Applied
    }

    /// Move a tab from one group to another (drag between strips).
    pub fn move_tab(&mut self, from: GroupId, tab: TabId, to: GroupId) -> Effect {
        if from == to {
            return Effect::Noop;
        }
        let Some(idx) = self.group(from).and_then(|g| g.tabs.iter().position(|t| t.id == tab))
        else {
            return Effect::Noop;
        };
        let t = self.group_mut(from).unwrap().tabs.remove(idx);
        // fix active in source
        if let Some(g) = self.group_mut(from) {
            if g.active == Some(tab) {
                g.active = g.tabs.first().map(|t| t.id);
            }
        }
        let Some(dest) = self.group_mut(to) else {
            // destination vanished: put the tab back where it was
            self.group_mut(from).map(|g| g.tabs.insert(idx, t));
            return Effect::Noop;
        };
        if dest.active.is_none() {
            dest.active = Some(t.id);
        }
        dest.tabs.push(t);
        Effect::Applied
    }

    pub fn rename(&mut self, id: GroupId, name: &str) -> Effect {
        let Some(g) = self.group_mut(id) else {
            return Effect::Noop;
        };
        if g.name == name {
            return Effect::Noop;
        }
        g.name = name.to_string();
        Effect::Applied
    }

    pub fn set_collapsed(&mut self, id: GroupId, collapsed: bool) -> Effect {
        let Some(g) = self.group_mut(id) else {
            return Effect::Noop;
        };
        if g.collapsed == collapsed {
            return Effect::Noop;
        }
        g.collapsed = collapsed;
        Effect::Applied
    }

    pub fn set_active(&mut self, id: GroupId, tab: TabId) -> Effect {
        let Some(g) = self.group_mut(id) else {
            return Effect::Noop;
        };
        if !g.tabs.iter().any(|t| t.id == tab) {
            return Effect::Noop;
        }
        if g.active == Some(tab) {
            return Effect::Noop;
        }
        g.active = Some(tab);
        Effect::Applied
    }

    /// Update a stored window's title (from a WinEvent name change).
    pub fn update_title(&mut self, hwnd: isize, title: &str) -> Effect {
        let mut applied = false;
        for g in self.groups.iter_mut() {
            for t in g.tabs.iter_mut() {
                if t.window.hwnd == hwnd && t.window.title != title {
                    t.window.title = title.to_string();
                    applied = true;
                }
            }
        }
        if applied {
            Effect::Applied
        } else {
            Effect::Noop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(hwnd: isize, exe: &str, title: &str) -> WindowIdentity {
        WindowIdentity {
            hwnd,
            pid: 1,
            exe_path: format!("C:\\{exe}"),
            exe_name: exe.to_string(),
            title: title.to_string(),
            class: "Class".to_string(),
            elevated: false,
        }
    }

    #[test]
    fn create_group_activates_first_window() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![win(100, "a.exe", "A"), win(200, "b.exe", "B")]);
        let group = s.group(g).unwrap();
        assert_eq!(group.tabs.len(), 2);
        assert_eq!(group.active, Some(group.tabs[0].id));
        assert!(!group.collapsed);
    }

    #[test]
    fn add_window_is_idempotent_across_groups() {
        let mut s = GroupSet::new();
        let g1 = s.create_group("One", vec![]);
        let g2 = s.create_group("Two", vec![]);
        assert_eq!(s.add_window(g1, win(100, "a.exe", "A")), Effect::Applied);
        // same hwnd again — no-op, even in the other group
        assert_eq!(s.add_window(g2, win(100, "a.exe", "A")), Effect::Noop);
        assert_eq!(s.group(g1).unwrap().tabs.len(), 1);
        assert_eq!(s.group(g2).unwrap().tabs.len(), 0);
    }

    #[test]
    fn remove_active_tab_promotes_next() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![win(100, "a.exe", "A"), win(200, "b.exe", "B")]);
        let active = s.group(g).unwrap().active.unwrap();
        assert_eq!(s.remove_tab(g, active), Effect::Applied);
        let group = s.group(g).unwrap();
        assert_ne!(group.active, Some(active));
        assert!(group.active.is_some());
    }

    #[test]
    fn stale_remove_is_noop() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![win(100, "a.exe", "A")]);
        assert_eq!(s.remove_tab(g, TabId(999)), Effect::Noop);
        assert_eq!(s.remove_tab(GroupId(999), TabId(0)), Effect::Noop);
    }

    #[test]
    fn window_destroyed_clears_tab_and_reactivates() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![win(100, "a.exe", "A"), win(200, "b.exe", "B")]);
        let active_hwnd = s.group(g).unwrap().tabs[0].window.hwnd;
        assert_eq!(s.window_destroyed(active_hwnd), Effect::Applied);
        let group = s.group(g).unwrap();
        assert_eq!(group.tabs.len(), 1);
        assert_eq!(group.tabs[0].window.hwnd, 200);
        assert_eq!(group.active, Some(group.tabs[0].id));
        // second event for the same hwnd — idempotent
        assert_eq!(s.window_destroyed(active_hwnd), Effect::Noop);
    }

    #[test]
    fn reorder_moves_tab() {
        let mut s = GroupSet::new();
        let g = s.create_group(
            "Dev",
            vec![win(100, "a.exe", "A"), win(200, "b.exe", "B"), win(300, "c.exe", "C")],
        );
        let first = s.group(g).unwrap().tabs[0].id;
        assert_eq!(s.reorder(g, first, 2), Effect::Applied);
        let order: Vec<isize> = s
            .group(g)
            .unwrap()
            .tabs
            .iter()
            .map(|t| t.window.hwnd)
            .collect();
        assert_eq!(order, vec![200, 300, 100]);
    }

    #[test]
    fn move_tab_between_groups() {
        let mut s = GroupSet::new();
        let g1 = s.create_group("One", vec![win(100, "a.exe", "A"), win(200, "b.exe", "B")]);
        let g2 = s.create_group("Two", vec![win(300, "c.exe", "C")]);
        let tab = s.group(g1).unwrap().tabs[0].id;
        assert_eq!(s.move_tab(g1, tab, g2), Effect::Applied);
        assert_eq!(s.group(g1).unwrap().tabs.len(), 1);
        assert_eq!(s.group(g2).unwrap().tabs.len(), 2);
        // source active was moved — reactivated to remaining tab
        assert!(s.group(g1).unwrap().active.is_some());
    }

    #[test]
    fn dissolve_never_returns_windows_but_clears_group() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![win(100, "a.exe", "A")]);
        assert_eq!(s.dissolve(g), Effect::Applied);
        assert_eq!(s.dissolve(g), Effect::Noop);
        assert!(s.groups().is_empty());
    }

    #[test]
    fn rename_and_collapse() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![]);
        assert_eq!(s.rename(g, "Study"), Effect::Applied);
        assert_eq!(s.rename(g, "Study"), Effect::Noop);
        assert_eq!(s.set_collapsed(g, true), Effect::Applied);
        assert_eq!(s.set_collapsed(g, true), Effect::Noop);
        assert_eq!(s.group(g).unwrap().name, "Study");
        assert!(s.group(g).unwrap().collapsed);
    }

    #[test]
    fn update_title_only_touches_matching_hwnd() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![win(100, "a.exe", "Old"), win(200, "b.exe", "B")]);
        assert_eq!(s.update_title(100, "New"), Effect::Applied);
        assert_eq!(s.update_title(100, "New"), Effect::Noop);
        assert_eq!(s.group(g).unwrap().tabs[0].window.title, "New");
        assert_eq!(s.group(g).unwrap().tabs[1].window.title, "B");
    }

    #[test]
    fn set_active_rejects_unknown_tab() {
        let mut s = GroupSet::new();
        let g = s.create_group("Dev", vec![win(100, "a.exe", "A")]);
        assert_eq!(s.set_active(g, TabId(42)), Effect::Noop);
        let tab = s.group(g).unwrap().tabs[0].id;
        assert_eq!(s.set_active(g, tab), Effect::Noop); // already active
        let mut s2 = GroupSet::new();
        let g2 = s2.create_group("X", vec![win(1, "a", "A"), win(2, "b", "B")]);
        let other = s2.group(g2).unwrap().tabs[1].id;
        assert_eq!(s2.set_active(g2, other), Effect::Applied);
    }
}
