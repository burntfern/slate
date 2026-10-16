use super::Controller;
use crate::core::Effect;
use crate::win::{
    activate_window, enumerate_windows, set_owned, window_title, StylingState, WinEvent,
    WinEventPump,
};

impl Controller {
    pub(super) fn drain_events(&mut self) -> bool {
        let mut changed = false;
        for event in WinEventPump::drain() {
            match event {
                WinEvent::Destroyed(hwnd)
                    if self.styling.contains_key(&hwnd) || self.groups.contains_window(hwnd) =>
                {
                    self.window_gone(hwnd);
                    changed = true;
                }
                WinEvent::TitleChanged(hwnd)
                    if self.styling.contains_key(&hwnd)
                        && !self.rename_overrides.contains_key(&hwnd) =>
                {
                    let title = window_title(hwnd);
                    if self.groups.update_title(hwnd, &title) == Effect::Applied {
                        changed = true;
                    }
                }
                WinEvent::MoveStarted(hwnd) => {
                    if Some(hwnd) != self.host_hwnd && !self.styling.contains_key(&hwnd) {
                        self.windows = enumerate_windows();
                        if self.trackable_drag(hwnd) {
                            self.drag = Some(hwnd);
                            self.drag_active = false;
                        }
                    }
                }
                WinEvent::MoveEnded(hwnd) if self.drag == Some(hwnd) => {
                    self.drag = None;
                    let drop = self.drag_active;
                    self.drag_active = false;
                    self.update_drag_highlight();
                    if drop {
                        self.add_window(hwnd);
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        changed
    }

    pub(super) fn window_gone(&mut self, hwnd: isize) {
        self.styling.remove(&hwnd);
        self.icons.remove(&hwnd);
        self.snapshots.remove(&hwnd);
        self.rename_overrides.remove(&hwnd);
        self.tab_colors.remove(&hwnd);
        if let Some(tab) = self.hover_preview.as_ref().map(|p| p.tab) {
            if self.active_group.and_then(|g| self.hwnd_of_tab(g, tab)) == Some(hwnd) {
                self.hide_hover_preview();
            }
        }
        self.windows.retain(|w| w.hwnd != hwnd);
        self.groups.window_destroyed(hwnd);
        if self.picker_open {
            self.refresh_picker();
        }
    }

    pub(super) fn activate_member(&mut self, hwnd: isize) {
        activate_window(hwnd);
    }

    pub(super) fn sync_worker(&mut self) {
        let host = self.host_hwnd.unwrap_or(0);
        if self.menu_overlay() {
            self.hide_hover_preview();
        } else if let Some(tab) = self.hover_preview.as_ref().map(|p| p.tab) {
            let hosted = self
                .active_group
                .and_then(|g| self.hwnd_of_tab(g, tab))
                .is_some_and(|h| self.styling.contains_key(&h));
            if !hosted {
                self.hide_hover_preview();
            }
        }
        let overlaying = self.overlaying();
        let active = if overlaying {
            None
        } else {
            self.active_group.and_then(|g| self.active_hwnd(g))
        };
        let hidden: Vec<isize> = self
            .styling
            .keys()
            .copied()
            .filter(|h| Some(*h) != active)
            .collect();
        self.worker.update(host, self.titlebar, active, hidden);
        let members: Vec<(isize, StylingState)> =
            self.styling.iter().map(|(k, v)| (*k, v.clone())).collect();
        self.janitor.persist(host, &members);
        set_owned(
            std::iter::once(host)
                .chain(self.styling.keys().copied())
                .filter(|hwnd| *hwnd != 0),
        );
    }
}
