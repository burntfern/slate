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
    let g = s.create_group(vec![win(100, "a.exe", "A"), win(200, "b.exe", "B")]);
    let group = s.group(g).unwrap();
    assert_eq!(group.tabs.len(), 2);
    assert_eq!(group.active, Some(group.tabs[0].id));
}

#[test]
fn add_window_is_idempotent_across_groups() {
    let mut s = GroupSet::new();
    let g1 = s.create_group(vec![]);
    let g2 = s.create_group(vec![]);
    assert_eq!(s.add_window(g1, win(100, "a.exe", "A")), Effect::Applied);
    assert_eq!(s.add_window(g2, win(100, "a.exe", "A")), Effect::Noop);
    assert_eq!(s.group(g1).unwrap().tabs.len(), 1);
    assert_eq!(s.group(g2).unwrap().tabs.len(), 0);
}

#[test]
fn remove_active_tab_promotes_next() {
    let mut s = GroupSet::new();
    let g = s.create_group(vec![win(100, "a.exe", "A"), win(200, "b.exe", "B")]);
    let active = s.group(g).unwrap().active.unwrap();
    assert_eq!(s.remove_tab(g, active), Effect::Applied);
    let group = s.group(g).unwrap();
    assert_ne!(group.active, Some(active));
    assert!(group.active.is_some());
}

#[test]
fn stale_remove_is_noop() {
    let mut s = GroupSet::new();
    let g = s.create_group(vec![win(100, "a.exe", "A")]);
    assert_eq!(s.remove_tab(g, TabId(999)), Effect::Noop);
    assert_eq!(s.remove_tab(GroupId(999), TabId(0)), Effect::Noop);
}

#[test]
fn window_destroyed_clears_tab_and_reactivates() {
    let mut s = GroupSet::new();
    let g = s.create_group(vec![win(100, "a.exe", "A"), win(200, "b.exe", "B")]);
    let active_hwnd = s.group(g).unwrap().tabs[0].window.hwnd;
    assert_eq!(s.window_destroyed(active_hwnd), Effect::Applied);
    let group = s.group(g).unwrap();
    assert_eq!(group.tabs.len(), 1);
    assert_eq!(group.tabs[0].window.hwnd, 200);
    assert_eq!(group.active, Some(group.tabs[0].id));
    assert_eq!(s.window_destroyed(active_hwnd), Effect::Noop);
}

#[test]
fn reorder_moves_tab() {
    let mut s = GroupSet::new();
    let g = s.create_group(vec![
        win(100, "a.exe", "A"),
        win(200, "b.exe", "B"),
        win(300, "c.exe", "C"),
    ]);
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
fn update_title_only_touches_matching_hwnd() {
    let mut s = GroupSet::new();
    let g = s.create_group(vec![win(100, "a.exe", "Old"), win(200, "b.exe", "B")]);
    assert_eq!(s.update_title(100, "New"), Effect::Applied);
    assert_eq!(s.update_title(100, "New"), Effect::Noop);
    assert_eq!(s.group(g).unwrap().tabs[0].window.title, "New");
    assert_eq!(s.group(g).unwrap().tabs[1].window.title, "B");
}

#[test]
fn set_active_rejects_unknown_tab() {
    let mut s = GroupSet::new();
    let g = s.create_group(vec![win(100, "a.exe", "A")]);
    assert_eq!(s.set_active(g, TabId(42)), Effect::Noop);
    let tab = s.group(g).unwrap().tabs[0].id;
    assert_eq!(s.set_active(g, tab), Effect::Noop);
    let mut s2 = GroupSet::new();
    let g2 = s2.create_group(vec![win(1, "a", "A"), win(2, "b", "B")]);
    let other = s2.group(g2).unwrap().tabs[1].id;
    assert_eq!(s2.set_active(g2, other), Effect::Applied);
}
