use ely_gpui_component::theme::{ActiveTheme, Mode, Theme};
use gpui::Context;

use super::dto::render_image;
use super::Controller;
use crate::config;
use crate::win::window_icon;

impl Controller {
    /// Show/hide the real window icon in each tab, extracting on first enable.
    pub(super) fn toggle_icons_in_tabs(&mut self) {
        self.settings.tabs.icons = !self.settings.tabs.icons;
        if self.settings.tabs.icons {
            for hwnd in self.styling.keys().copied().collect::<Vec<_>>() {
                self.ensure_icon(hwnd);
            }
        }
        self.save_settings();
    }

    /// Show/hide the tab-count badge on the overflow chevron.
    pub(super) fn toggle_tab_badge(&mut self) {
        self.settings.tabs.count_badge = !self.settings.tabs.count_badge;
        self.save_settings();
    }

    /// Persist the current settings (best-effort; a write failure is silent).
    pub(super) fn save_settings(&self) {
        config::save(&self.settings);
    }

    /// Apply the saved theme. `"system"` (or anything unknown) follows the OS.
    pub(super) fn apply_theme(&self, cx: &mut Context<Self>) {
        let system = Mode::from(cx.window_appearance());
        Theme::set_mode_now(self.resolve_mode(system), cx);
    }

    /// The stored theme choice resolved to a concrete mode.
    fn resolve_mode(&self, system: Mode) -> Mode {
        match self.settings.theme.as_str() {
            "dark" => Mode::Dark,
            "light" => Mode::Light,
            _ => system,
        }
    }

    /// Flip light/dark, apply it, and persist the new explicit choice (a toggle
    /// leaves `"system"` behind — the user has now stated a preference).
    pub(super) fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        let next = match cx.theme().mode() {
            Mode::Dark => Mode::Light,
            Mode::Light => Mode::Dark,
        };
        self.settings.theme = match next {
            Mode::Dark => "dark".to_string(),
            Mode::Light => "light".to_string(),
        };
        Theme::set_mode_now(next, cx);
        self.save_settings();
    }

    /// Extract and cache a window's icon the first time it is needed.
    pub(super) fn ensure_icon(&mut self, hwnd: isize) {
        if self.icons.contains_key(&hwnd) {
            return;
        }
        let icon = window_icon(hwnd).and_then(render_image);
        self.icons.insert(hwnd, icon);
    }
}
