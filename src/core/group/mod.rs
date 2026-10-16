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

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Applied,
    Noop,
}

#[derive(Debug, Default)]
pub struct GroupSet {
    groups: Vec<Group>,
    next_group: u64,
    next_tab: u64,
    generation: u64,
}

impl GroupSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn group_mut(&mut self, id: GroupId) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    pub fn contains_window(&self, hwnd: isize) -> bool {
        self.groups
            .iter()
            .any(|g| g.tabs.iter().any(|t| t.window.hwnd == hwnd))
    }

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
        self.bump();
        id
    }

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
        self.bump();
        Effect::Applied
    }

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
        self.bump();
        Effect::Applied
    }

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
            self.bump();
            Effect::Applied
        } else {
            Effect::Noop
        }
    }

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
        self.bump();
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
        self.bump();
        Effect::Applied
    }

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
            self.bump();
            Effect::Applied
        } else {
            Effect::Noop
        }
    }
}

#[cfg(test)]
mod tests;
