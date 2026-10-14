use super::identity::{GroupId, TabId};
use serde::{Deserialize, Serialize};

/// Persisted form of a group. HWNDs are never saved — windows are matched on
/// restore by exe path + title hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTab {
    pub exe_path: String,
    pub title_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedGroup {
    pub name: String,
    pub collapsed: bool,
    pub active_index: usize,
    pub tabs: Vec<SavedTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
    pub updated_at: u64,
    pub groups: Vec<SavedGroup>,
}

/// A window currently open on the desktop, as known to the restore planner.
#[derive(Debug, Clone)]
pub struct OpenWindow {
    pub hwnd: isize,
    pub exe_path: String,
    pub title: String,
}

/// What to do with one saved tab during restore.
#[derive(Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// Matched an open window — group it.
    Matched { hwnd: isize, group: usize, tab: usize },
    /// No open window matched — surface in the recovery dialog. Never launch.
    Missing { group: usize, tab: usize },
}

/// Pure planner: match saved tabs against open windows. Matching rule:
/// exe path equality (case-insensitive) and, if several candidates exist,
/// the one whose title contains the hint.
pub fn plan_restore(ws: &Workspace, open: &[OpenWindow]) -> Vec<RestoreOutcome> {
    let mut out = Vec::new();
    let mut used = std::collections::HashSet::new();
    for (gi, g) in ws.groups.iter().enumerate() {
        for (ti, t) in g.tabs.iter().enumerate() {
            let want = t.exe_path.to_lowercase();
            let mut candidate = open.iter().find(|w| {
                !used.contains(&w.hwnd) && w.exe_path.to_lowercase() == want
            });
            if candidate.is_none() && !t.title_hint.is_empty() {
                candidate = open.iter().find(|w| {
                    !used.contains(&w.hwnd)
                        && w.title
                            .to_lowercase()
                            .contains(&t.title_hint.to_lowercase())
                });
            }
            match candidate {
                Some(w) => {
                    used.insert(w.hwnd);
                    out.push(RestoreOutcome::Matched {
                        hwnd: w.hwnd,
                        group: gi,
                        tab: ti,
                    });
                }
                None => out.push(RestoreOutcome::Missing { group: gi, tab: ti }),
            }
        }
    }
    out
}

/// Convenience view used by the UI to build a GroupSet from a plan.
pub struct PlannedGroup {
    pub group_id: GroupId,
    pub active_tab: Option<TabId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace {
            id: 1,
            name: "Study".into(),
            updated_at: 0,
            groups: vec![SavedGroup {
                name: "Reading".into(),
                collapsed: false,
                active_index: 0,
                tabs: vec![
                    SavedTab { exe_path: "C:\\A.exe".into(), title_hint: String::new() },
                    SavedTab { exe_path: "C:\\missing.exe".into(), title_hint: String::new() },
                ],
            }],
        }
    }

    fn open(hwnd: isize, exe: &str, title: &str) -> OpenWindow {
        OpenWindow { hwnd, exe_path: exe.into(), title: title.into() }
    }

    #[test]
    fn matches_by_exe_and_reports_missing() {
        let out = plan_restore(&ws(), &[open(100, "c:\\a.exe", "Hello")]);
        assert_eq!(
            out,
            vec![
                RestoreOutcome::Matched { hwnd: 100, group: 0, tab: 0 },
                RestoreOutcome::Missing { group: 0, tab: 1 },
            ]
        );
    }

    #[test]
    fn never_matches_same_window_twice() {
        let out = plan_restore(&ws(), &[open(100, "c:\\a.exe", "x")]);
        // second saved tab (missing.exe) must not steal the only candidate
        assert!(matches!(out[1], RestoreOutcome::Missing { .. }));
    }

    #[test]
    fn title_hint_fallback() {
        let mut w = ws();
        w.groups[0].tabs[0].exe_path = "C:\\gone.exe".into();
        w.groups[0].tabs[0].title_hint = "Project".into();
        let out = plan_restore(&w, &[open(55, "C:\\other.exe", "My Project window")]);
        assert_eq!(out[0], RestoreOutcome::Matched { hwnd: 55, group: 0, tab: 0 });
    }

    #[test]
    fn empty_open_reports_all_missing() {
        let out = plan_restore(&ws(), &[]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|o| matches!(o, RestoreOutcome::Missing { .. })));
    }
}
