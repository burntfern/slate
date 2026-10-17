use gpui::Context;
use std::time::Duration;

use super::dto::{render_snapshot, HoverPreview};
use super::{Controller, PREVIEW_LINGER_MS, PREVIEW_OPEN_MS};
use crate::core::TabId;
use crate::win::capture_window;

impl Controller {
    pub(super) fn overlaying(&self) -> bool {
        self.menu_overlay() || self.hover_preview.is_some()
    }

    pub(super) fn menu_overlay(&self) -> bool {
        self.overlay
            || self.tab_menu_open
            || self.picker_open
            || self.saved_menu_open
            || self.tab_menu.is_some()
            || self.renaming.is_some()
            || self.saved_prompt.is_some()
    }

    pub(super) fn preview_hover(
        &mut self,
        tab: TabId,
        anchor: (f32, f32, f32, f32),
        cx: &mut Context<Self>,
    ) {
        if !self.settings.tabs.hover_preview {
            return;
        }
        self.invalidate_info();
        self.hover_tab = Some(tab);
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if self.hover_preview.is_some() {
            self.show_hover_preview(tab, anchor, cx);
            return;
        }
        self.after_hover_delay(PREVIEW_OPEN_MS, cx, move |c, cx| {
            c.show_hover_preview(tab, anchor, cx);
            c.sync_worker();
            cx.notify();
        });
    }

    pub(super) fn preview_unhover(&mut self, tab: TabId, cx: &mut Context<Self>) {
        if self.hover_tab != Some(tab) {
            return;
        }
        self.invalidate_info();
        self.hover_tab = None;
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if self.hover_preview.is_none() {
            return;
        }
        self.after_hover_delay(PREVIEW_LINGER_MS, cx, |c, cx| {
            if !c.card_hovered {
                c.hide_hover_preview();
                c.sync_worker();
                cx.notify();
            }
        });
    }

    pub(super) fn preview_card_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.invalidate_info();
        self.card_hovered = hovered;
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if hovered || self.hover_preview.is_none() || self.hover_tab.is_some() {
            return;
        }
        self.after_hover_delay(PREVIEW_LINGER_MS, cx, |c, cx| {
            if !c.card_hovered && c.hover_tab.is_none() {
                c.hide_hover_preview();
                c.sync_worker();
                cx.notify();
            }
        });
    }

    pub(super) fn toggle_hover_preview(&mut self) {
        self.invalidate_info();
        self.settings.tabs.hover_preview = !self.settings.tabs.hover_preview;
        if !self.settings.tabs.hover_preview {
            self.hover_tab = None;
            self.card_hovered = false;
            self.preview_gen = self.preview_gen.wrapping_add(1);
            self.hide_hover_preview();
        }
        self.save_settings();
    }

    fn after_hover_delay(
        &mut self,
        ms: u64,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let gen = self.preview_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(ms))
                .await;
            let _ = this.update(cx, |c, cx| {
                if c.preview_gen == gen {
                    f(c, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn show_hover_preview(
        &mut self,
        tab: TabId,
        anchor: (f32, f32, f32, f32),
        cx: &mut Context<Self>,
    ) {
        self.invalidate_info();
        let (Some(_host), Some(group)) = (self.host_hwnd, self.active_group) else {
            return;
        };
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        let active = self.active_hwnd(group);
        let menu_open = self.menu_overlay();
        let cached = self.snapshots.get(&hwnd).cloned();
        self.hover_preview = Some(HoverPreview {
            tab,
            anchor,
            image: cached,
        });
        if Some(hwnd) != active || menu_open {
            return;
        }
        let gen = self.preview_gen;
        cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async move { capture_window(hwnd) })
                .await;
            let _ = this.update(cx, |controller, cx| {
                if controller.preview_gen != gen {
                    return;
                }
                let image = snapshot.and_then(render_snapshot);
                if let Some(image) = image.as_ref() {
                    controller.snapshots.insert(hwnd, image.clone());
                }
                let fallback = controller.snapshots.get(&hwnd).cloned();
                if let Some(preview) = controller.hover_preview.as_mut() {
                    preview.image = image.or(fallback);
                }
                controller.invalidate_info();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn hide_hover_preview(&mut self) {
        self.invalidate_info();
        self.hover_preview = None;
        self.card_hovered = false;
    }
}
