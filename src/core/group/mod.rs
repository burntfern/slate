use super::identity::{GroupId, TabId, WindowIdentity};

#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub window: WindowIdentity,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub id: GroupId,
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
    pub fn create_group(&mut self, windows: Vec<WindowIdentity>) -> GroupId {
        let id = GroupId(self.next_group);
        self.next_group += 1;
        let mut tabs = Vec::new();
        for w in windows {
            let tid = TabId(self.next_tab);
            self.next_tab += 1;
            tabs.push(Tab { id: tid, window: w });
        }
        let active = tabs.first().map(|t| t.id);
        self.groups.push(Group { id, tabs, active });
        id
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
            let Some(pos) = g.tabs.iter().position(|t| t.window.hwnd == hwnd) else {
                continue;
            };
            let was_active = g.active == Some(g.tabs[pos].id);
            g.tabs.remove(pos);
            applied = true;
            if was_active {
                g.active = g.tabs.first().map(|t| t.id);
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
mod tests;
