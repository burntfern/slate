use super::dto::PickerRow;
use super::Controller;
use crate::win::is_blacklisted;

impl Controller {
    pub(super) fn set_picker_open(&mut self, open: bool) {
        if open {
            self.close_other_dropdowns();
            self.refresh_picker();
        }
        self.picker_open = open;
    }

    pub(super) fn refresh_picker(&mut self) {
        let hosted: Vec<isize> = self.styling.keys().copied().collect();
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
        rows.sort_by(|a, b| {
            a.reason
                .is_some()
                .cmp(&b.reason.is_some())
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        self.picker_rows = rows;
    }
}
