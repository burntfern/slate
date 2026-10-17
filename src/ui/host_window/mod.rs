use std::sync::Arc;

use ely_gpui_component::feedback::ToastViewport;
use ely_gpui_component::forms::TextInput;
use ely_gpui_component::theme::ActiveTheme;
use ely_gpui_component::typography::{Caption, Kbd};
use gpui::prelude::*;
use gpui::{
    div, App, Context, Entity, IntoElement, MouseButton, ParentElement, Render, RenderImage,
    ScrollHandle, Size, Styled, Subscription, Window,
};

use crate::controller::{Cmd, Controller, SavedPrompt, TabInfo};
use crate::core::{GroupId, TabId};
use crate::ui::tab_bar::{SlateTab, SlateTabBar, TabDrag, TabIcon};

mod art;
mod dialogs;
mod hover_card;
mod overlays;
mod panels;
mod title_bar;

use art::{empty_state, grayscale_mark, key_cap};
use hover_card::hover_card;

fn hwnd_of(window: &Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
        _ => None,
    }
}

pub struct HostWindow {
    controller: Entity<Controller>,
    ready: bool,
    close_routed: bool,
    empty_icon: Option<Arc<RenderImage>>,
    rename_field: Option<Entity<TextInput>>,
    rename_primed: Option<TabId>,
    saved_field: Option<Entity<TextInput>>,
    saved_primed: Option<SavedPrompt>,
    tab_scroll: ScrollHandle,
    _observe: Subscription,
    _observe_controller: Subscription,
}

pub(super) struct HostCtx<'a> {
    pub(super) window: &'a mut Window,
    pub(super) cx: &'a mut App,
    pub(super) controller: Entity<Controller>,
    pub(super) info: &'a crate::controller::HostInfo,
    pub(super) group: Option<GroupId>,
    pub(super) viewport: Size<gpui::Pixels>,
    pub(super) palette: ely_gpui_component::theme::Palette,
}

impl HostWindow {
    pub fn new(controller: Entity<Controller>, cx: &mut Context<Self>) -> Self {
        let toaster = controller.read(cx).toaster();
        let _observe = cx.observe(&toaster, |_this, _toaster, cx| cx.notify());
        let _observe_controller = cx.observe(&controller, |_this, _controller, cx| cx.notify());
        Self {
            controller,
            ready: false,
            close_routed: false,
            empty_icon: grayscale_mark(),
            rename_field: None,
            rename_primed: None,
            saved_field: None,
            saved_primed: None,
            tab_scroll: ScrollHandle::new(),
            _observe,
            _observe_controller,
        }
    }
}

impl Render for HostWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controller = self.controller.clone();

        if !self.ready {
            if let Some(hwnd) = hwnd_of(window) {
                let titlebar_h = {
                    let theme = cx.theme();
                    theme
                        .titlebar_height()
                        .to_pixels(window.rem_size())
                        .as_f32()
                };
                controller.update(cx, |c, cx| {
                    c.dispatch(
                        Cmd::HostReady {
                            hwnd,
                            titlebar: titlebar_h,
                        },
                        cx,
                    );
                });
                self.ready = true;
            }
        }

        if !self.close_routed {
            let ctrl = controller.clone();
            window.on_window_should_close(cx, move |window, cx| {
                ctrl.update(cx, |c, cx| c.dispatch(Cmd::Shutdown, cx));
                window.remove_window();
                false
            });
            self.close_routed = true;
        }

        let (titlebar_h, bg) = {
            let theme = cx.theme();
            (theme.titlebar_height(), theme.colors.bg)
        };

        let info = self.controller.read_with(cx, |c, _| c.host_info());
        let group = info.group;

        let mut tab_bar = SlateTabBar::new("host-tabs");
        let palette = cx.theme().palette();
        for TabInfo {
            id,
            title,
            icon,
            accent,
            ..
        } in &info.tabs
        {
            let icon = match (info.icons_in_tabs, icon) {
                (true, Some(image)) => TabIcon::Real(image.clone()),
                _ => TabIcon::Glyph(ely_gpui_component::primitives::IconName::AppWindow),
            };
            let color = accent.map(|index| palette.hue(index, "tab accent"));
            tab_bar =
                tab_bar.tab(SlateTab::new(id.0.to_string(), title.clone(), icon).accent(color));
        }
        if let Some(active) = &info.active {
            tab_bar = tab_bar.selected(gpui::SharedString::from(active.0.to_string()));
        }
        tab_bar = tab_bar.on_select({
            let ctrl = controller.clone();
            move |id, _w, cx| {
                if let (Some(g), Ok(n)) = (group, id.to_string().parse::<u64>()) {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(
                            Cmd::SelectTab {
                                group: g,
                                tab: TabId(n),
                            },
                            cx,
                        );
                    });
                }
            }
        });
        tab_bar = tab_bar.on_close({
            let ctrl = controller.clone();
            move |id, _w, cx| {
                if let (Some(g), Ok(n)) = (group, id.to_string().parse::<u64>()) {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(
                            Cmd::Ungroup {
                                group: g,
                                tab: TabId(n),
                            },
                            cx,
                        );
                    });
                }
            }
        });
        tab_bar = tab_bar.on_reorder({
            let ctrl = controller.clone();
            move |from, to, _w, cx| {
                if let Some(g) = group {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::ReorderTab { group: g, from, to }, cx);
                    });
                }
            }
        });
        tab_bar = tab_bar.on_context({
            let ctrl = controller.clone();
            move |id, position, _w, cx| {
                if let (Some(_g), Ok(n)) = (group, id.to_string().parse::<u64>()) {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(
                            Cmd::OpenTabMenu {
                                tab: TabId(n),
                                at: (position.x.as_f32(), position.y.as_f32()),
                            },
                            cx,
                        );
                    });
                }
            }
        });
        tab_bar = tab_bar.on_tab_hover({
            let ctrl = controller.clone();
            move |id, hovered, bounds, _window, cx| {
                let (Some(_g), Ok(n)) = (group, id.to_string().parse::<u64>()) else {
                    return;
                };
                let tab = TabId(n);
                if hovered {
                    let anchor = (
                        bounds.origin.x.as_f32(),
                        bounds.origin.y.as_f32(),
                        bounds.size.width.as_f32(),
                        bounds.size.height.as_f32(),
                    );
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::PreviewTab { tab, anchor }, cx);
                    });
                } else {
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::PreviewHide { tab }, cx));
                }
            }
        });

        let viewport = window.viewport_size();

        let title = {
            let mut ctx = HostCtx {
                window: &mut *window,
                cx: &mut *cx,
                controller: controller.clone(),
                info: info.as_ref(),
                group,
                viewport,
                palette,
            };
            title_bar::build(&mut ctx, tab_bar, titlebar_h, bg, &self.tab_scroll)
        };

        let dismiss = {
            let ctrl = controller.clone();
            let (overlay, tab_menu, picker, saved, ctx_menu) = (
                info.overlay,
                info.tab_menu_open,
                info.picker_open,
                info.saved_menu_open,
                info.tab_menu.is_some(),
            );
            move |_ev: &gpui::MouseDownEvent, _w: &mut Window, cx: &mut App| {
                if !(overlay || tab_menu || picker || saved || ctx_menu) {
                    return;
                }
                ctrl.update(cx, |c, cx| {
                    if overlay {
                        c.dispatch(Cmd::SetOverlay(false), cx);
                    }
                    if tab_menu {
                        c.dispatch(Cmd::SetTabMenu(false), cx);
                    }
                    if picker {
                        c.dispatch(Cmd::SetPickerOpen(false), cx);
                    }
                    if saved {
                        c.dispatch(Cmd::SetSavedMenu(false), cx);
                    }
                    if ctx_menu {
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    }
                });
            }
        };

        let escape = {
            let ctrl = controller.clone();
            let (overlay, tab_menu, picker, saved, renaming, prompt) = (
                info.overlay,
                info.tab_menu_open,
                info.picker_open,
                info.saved_menu_open,
                info.renaming.is_some(),
                info.saved_prompt.is_some(),
            );
            move |ev: &gpui::KeyDownEvent, _w: &mut Window, cx: &mut App| {
                if ev.keystroke.key != "escape" {
                    return;
                }
                if !(overlay || tab_menu || picker || saved || renaming || prompt) {
                    return;
                }
                ctrl.update(cx, |c, cx| {
                    if overlay {
                        c.dispatch(Cmd::SetOverlay(false), cx);
                    }
                    if tab_menu {
                        c.dispatch(Cmd::SetTabMenu(false), cx);
                    }
                    if picker {
                        c.dispatch(Cmd::SetPickerOpen(false), cx);
                    }
                    if saved {
                        c.dispatch(Cmd::SetSavedMenu(false), cx);
                    }
                    if renaming {
                        c.dispatch(Cmd::CancelRename, cx);
                    }
                    if prompt {
                        c.dispatch(Cmd::CancelSavedPrompt, cx);
                    }
                });
            }
        };

        let detach_out = {
            let ctrl = controller.clone();
            move |drag: &TabDrag, window: &mut Window, cx: &mut App| {
                let origin = window.bounds().origin;
                let mouse = window.mouse_position();
                let screen_x = origin.x.as_f32() + mouse.x.as_f32();
                let screen_y = origin.y.as_f32() + mouse.y.as_f32();
                ctrl.update(cx, |c, cx| {
                    c.dispatch(
                        Cmd::DetachTabOut {
                            from: drag.from,
                            screen_x,
                            screen_y,
                        },
                        cx,
                    );
                });
            }
        };

        let content = div()
            .id("host-content")
            .size_full()
            .flex()
            .flex_col()
            .bg(bg)
            .on_mouse_down(MouseButton::Left, dismiss)
            .capture_key_down(escape)
            .on_drop::<TabDrag>(detach_out)
            .child(title.element);

        let overlaying = info.overlay
            || info.tab_menu_open
            || info.picker_open
            || info.saved_menu_open
            || info.tab_menu.is_some()
            || info.renaming.is_some()
            || info.saved_prompt.is_some();
        let body = match (info.tabs.is_empty(), overlaying) {
            (true, _) => {
                let caption = Caption::new("No windows yet").into_any_element();
                let hint = div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child("Click")
                    .child(Kbd::new("+"))
                    .child("to add a window, or")
                    .child(key_cap("Drag", cx))
                    .child("one in while holding")
                    .child(key_cap("Ctrl", cx))
                    .into_any_element();
                empty_state(cx, self.empty_icon.clone(), caption, hint)
            }
            (false, true) => {
                let caption = Caption::new(info.caption.clone()).into_any_element();
                let hint = div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child("Press")
                    .child(Kbd::new("escape"))
                    .child("to bring them back")
                    .into_any_element();
                empty_state(cx, self.empty_icon.clone(), caption, hint)
            }
            (false, false) => div().flex_1(),
        };

        let toaster = self.controller.read(cx).toaster();
        let mut content = content.child(body);
        if let Some(preview) = info.hover_preview.as_ref() {
            let tab = info.tabs.iter().find(|t| t.id == preview.tab);
            let on_hover = {
                let ctrl = controller.clone();
                move |hovered: &bool, _w: &mut Window, cx: &mut App| {
                    let hovered = *hovered;
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::PreviewCardHover(hovered), cx));
                }
            };
            content = content.child(hover_card(cx, preview, tab, viewport, on_hover));
        }
        if let Some(overlay) = title.picker_overlay {
            content = content.child(overlay);
        }
        if let Some(overlay) = title.chevron_overlay {
            content = content.child(overlay);
        }
        if let Some(overlay) = title.tab_menu_overlay {
            content = content.child(overlay);
        }
        if let Some(overlay) = title.saved_overlay {
            content = content.child(overlay);
        }
        if let Some(dialog) = self.rename_dialog(window, cx, info.as_ref(), group) {
            content = content.child(dialog);
        }
        if let Some(dialog) = self.saved_dialog(window, cx, info.as_ref()) {
            content = content.child(dialog);
        }
        content
            .child(ToastViewport::new("host-toasts", &toaster))
            .into_any_element()
    }
}
