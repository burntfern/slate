use gpui::Context;
use std::time::Duration;

use super::dto::pick_toast;
use super::Controller;
use crate::win::{
    content_rect_screen, ctrl_held, enumerate_windows, is_blacklisted, window_rect,
    window_under_cursor, Highlight, KeyboardCapture, PickKey,
};

impl Controller {
    pub(super) fn begin_pick(&mut self, cx: &mut Context<Self>) {
        self.windows = enumerate_windows();
        self.end_pick();
        self.picking = true;
        self.capture = KeyboardCapture::install();
        if self.highlight.is_none() {
            self.highlight = Highlight::new();
        }
        self.toaster.update(cx, |toaster, cx| {
            toaster.push(pick_toast(), cx);
        });
        self.pick_gen = self.pick_gen.wrapping_add(1);
        let generation = self.pick_gen;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            let done = this
                .update(cx, |controller, cx| controller.poll_pick(generation, cx))
                .unwrap_or(true);
            if done {
                break;
            }
        })
        .detach();
    }

    fn poll_pick(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.picking || generation != self.pick_gen {
            return true;
        }
        let keys = self
            .capture
            .as_ref()
            .map(KeyboardCapture::drain)
            .unwrap_or_default();
        if let Some(key) = keys.into_iter().next() {
            match key {
                PickKey::Tab => self.commit_pick(),
                PickKey::Escape => self.end_pick(),
            }
            self.sync_worker();
            cx.notify();
            return true;
        }
        let hovered =
            window_under_cursor().filter(|hwnd| self.windows.iter().any(|w| w.hwnd == *hwnd));
        if self.hovered != hovered {
            self.hovered = hovered;
            self.update_highlight();
            cx.notify();
        }
        false
    }

    fn update_highlight(&self) {
        let Some(highlight) = self.highlight.as_ref() else {
            return;
        };
        match self.hovered.and_then(window_rect) {
            Some((x, y, w, h)) => highlight.show(x, y, w, h),
            None => highlight.hide(),
        }
    }

    pub(super) fn trackable_drag(&self, hwnd: isize) -> bool {
        if Some(hwnd) == self.host_hwnd || self.styling.contains_key(&hwnd) {
            return false;
        }
        self.windows
            .iter()
            .find(|w| w.hwnd == hwnd)
            .is_some_and(|w| !is_blacklisted(&w.class) && !w.elevated)
    }

    fn drag_target_ok(&self, hwnd: isize) -> bool {
        if !ctrl_held() {
            return false;
        }
        let Some(host) = self.host_hwnd else {
            return false;
        };
        let Some((hx, hy, hw, hh)) = content_rect_screen(host, self.titlebar) else {
            return false;
        };
        let Some((wx, wy, ww, wh)) = window_rect(hwnd) else {
            return false;
        };
        wx < hx + hw && wx + ww > hx && wy < hy + hh && wy + wh > hy
    }

    pub(super) fn pump_drag(&mut self) {
        let Some(hwnd) = self.drag else {
            return;
        };
        if window_rect(hwnd).is_none() {
            self.drag = None;
            self.drag_active = false;
            self.update_drag_highlight();
            return;
        }
        let active = self.drag_target_ok(hwnd);
        if active != self.drag_active {
            self.drag_active = active;
            self.update_drag_highlight();
        }
    }

    pub(super) fn update_drag_highlight(&mut self) {
        if self.drag_active {
            if self.highlight.is_none() {
                self.highlight = Highlight::new();
            }
            if let (Some(highlight), Some(host)) = (self.highlight.as_ref(), self.host_hwnd) {
                if let Some((x, y, w, h)) = content_rect_screen(host, self.titlebar) {
                    highlight.show(x, y, w, h);
                }
            }
        } else if let Some(highlight) = self.highlight.as_ref() {
            highlight.hide();
        }
    }

    fn commit_pick(&mut self) {
        let target = self.hovered.take();
        self.end_pick();
        if let Some(hwnd) = target {
            self.add_window(hwnd);
        }
    }

    pub(super) fn end_pick(&mut self) {
        self.picking = false;
        self.hovered = None;
        self.capture = None;
        if let Some(highlight) = self.highlight.as_ref() {
            highlight.hide();
        }
        self.pick_gen = self.pick_gen.wrapping_add(1);
    }
}
