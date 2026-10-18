use super::dto::PickerRow;
use super::Controller;
use crate::win::{enumerate_windows, is_blacklisted};
use std::collections::HashSet;

impl Controller {
    pub(super) fn set_picker_open(&mut self, open: bool) {
        if open {
            self.close_other_dropdowns();
            self.windows = enumerate_windows();
            self.refresh_picker();
        }
        self.picker_open = open;
    }

    pub(super) fn refresh_picker(&mut self) {
        let hosted: HashSet<isize> = self.styling.keys().copied().collect();
        let eligible: Vec<isize> = self
            .windows
            .iter()
            .filter(|w| !is_blacklisted(&w.class) && !w.elevated && !hosted.contains(&w.hwnd))
            .map(|w| w.hwnd)
            .collect();
        for hwnd in eligible {
            self.ensure_icon(hwnd);
        }
        let mut rows: Vec<PickerRow> = self
            .windows
            .iter()
            .map(|w| {
                let reason = if is_blacklisted(&w.class) {
                    Some("This window can't be hosted".to_string())
                } else if w.elevated {
                    Some("Run Slate as administrator to host this".to_string())
                } else if hosted.contains(&w.hwnd) {
                    Some("Already hosted".to_string())
                } else {
                    None
                };
                PickerRow {
                    hwnd: w.hwnd,
                    title: if w.title.is_empty() {
                        w.exe_name.clone()
                    } else {
                        w.title.clone()
                    },
                    exe: w.exe_name.clone(),
                    icon: self.icons.get(&w.hwnd).cloned().flatten(),
                    reason,
                }
            })
            .collect();
        rows.sort_by_cached_key(|row| (row.reason.is_some(), row.title.to_lowercase()));
        self.picker_rows = rows;
    }
}
