use gpui::Context;
use std::time::Duration;

use super::dto::{render_snapshot, HoverPreview};
use super::{Controller, PREVIEW_LINGER_MS, PREVIEW_OPEN_MS};
use crate::core::TabId;
use crate::win::capture_window;

impl Controller {
    /// True while any surface hides the hosted member: menu, dropdowns, picker,
    /// rename prompt, or hover card. An owned member paints above the host, so
    /// these are the states that blank the content rect.
    pub(super) fn overlaying(&self) -> bool {
        self.menu_overlay() || self.hover_preview.is_some()
    }

    /// The menu-ish overlays only — everything that hides the member for a
    /// reason other than the hover card itself.
    pub(super) fn menu_overlay(&self) -> bool {
        self.overlay
            || self.tab_menu_open
            || self.picker_open
            || self.saved_menu_open
            || self.tab_menu.is_some()
            || self.renaming.is_some()
            || self.saved_prompt.is_some()
    }

    /// The pointer rested on a tab: open the card under it on ely's `HoverCard`
    /// rhythm. While the card is already up, moving to another tab switches it
    /// with no delay.
    pub(super) fn preview_hover(
        &mut self,
        tab: TabId,
        anchor: (f32, f32, f32, f32),
        cx: &mut Context<Self>,
    ) {
        if !self.settings.tabs.hover_preview {
            return;
        }
        self.hover_tab = Some(tab);
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if self.hover_preview.is_some() {
            self.show_hover_preview(tab, anchor);
            return;
        }
        self.after_hover_delay(PREVIEW_OPEN_MS, cx, move |c, cx| {
            c.show_hover_preview(tab, anchor);
            c.sync_worker();
            cx.notify();
        });
    }

    /// Pointer left `tab`: linger so the card can be crossed, then drop it. A
    /// stale leave (the pointer is already on another tab) is ignored.
    pub(super) fn preview_unhover(&mut self, tab: TabId, cx: &mut Context<Self>) {
        if self.hover_tab != Some(tab) {
            return;
        }
        self.hover_tab = None;
        self.preview_gen = self.preview_gen.wrapping_add(1);
        if self.hover_preview.is_none() {
            return;
        }
        self.after_hover_delay(PREVIEW_LINGER_MS, cx, |c, cx| {
            // The pointer may have crossed into the card, which keeps it open.
            if !c.card_hovered {
                c.hide_hover_preview();
                c.sync_worker();
                cx.notify();
            }
        });
    }

    /// The pointer entered (`true`) or left (`false`) the card. Entering cancels
    /// a pending hide; leaving starts the linger unless the pointer is back on a
    /// tab (whose own hover takes over).
    pub(super) fn preview_card_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
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

    /// Show/hide the hover preview card. Hiding it also drops any card on screen
    /// and forgets the hover state.
    pub(super) fn toggle_hover_preview(&mut self) {
        self.settings.tabs.hover_preview = !self.settings.tabs.hover_preview;
        if !self.settings.tabs.hover_preview {
            self.hover_tab = None;
            self.card_hovered = false;
            self.preview_gen = self.preview_gen.wrapping_add(1);
            self.hide_hover_preview();
        }
        self.save_settings();
    }

    /// Run `f` after `ms`, unless a later hover change bumps the generation
    /// first — which cancels this pending open or linger.
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

    /// Show the card. The frame is sampled here — before opening, which hides the
    /// member — and only for the active window; inactive ones fall back to the
    /// last frame we caught.
    pub(super) fn show_hover_preview(&mut self, tab: TabId, anchor: (f32, f32, f32, f32)) {
        let (Some(_host), Some(group)) = (self.host_hwnd, self.active_group) else {
            return;
        };
        let Some(hwnd) = self.hwnd_of_tab(group, tab) else {
            return;
        };
        let active = self.active_hwnd(group);
        let image = if Some(hwnd) == active && !self.overlaying() {
            let captured = capture_window(hwnd).and_then(render_snapshot);
            if let Some(frame) = captured.as_ref() {
                self.snapshots.insert(hwnd, frame.clone());
            }
            captured.or_else(|| self.snapshots.get(&hwnd).cloned())
        } else {
            self.snapshots.get(&hwnd).cloned()
        };
        self.hover_preview = Some(HoverPreview { tab, anchor, image });
    }

    /// Drop the card; the member comes back on the next `sync_worker`.
    pub(super) fn hide_hover_preview(&mut self) {
        self.hover_preview = None;
        self.card_hovered = false;
    }
}
