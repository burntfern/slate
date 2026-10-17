use std::rc::Rc;

use ely_gpui_component::buttons::IconButton;
use ely_gpui_component::primitives::{Icon, IconName, Image};
use ely_gpui_component::theme::{
    ActiveTheme, ControlSize, Elevation, IconSize, Radius, TextSize, HUE_NAMES,
};
use ely_gpui_component::typography::{Ellipsis, Kbd};
use gpui::prelude::*;
use gpui::{
    canvas, div, px, AnyElement, Bounds, ClipboardItem, FontWeight, MouseButton, Point, Styled,
    Window,
};

use super::overlays::{enter_animation, overlay_layer, Action};
use super::HostCtx;
use crate::controller::Cmd;

pub(super) fn tab_overflow(ctx: &mut HostCtx) -> (AnyElement, Option<AnyElement>) {
    let group = ctx.group;
    let viewport = ctx.viewport;
    let anchor =
        ctx.window
            .use_keyed_state(("host-tab-overflow-anchor", 0usize), ctx.cx, |_, _| {
                Bounds::default()
            });
    let info = ctx.info;
    let controller = ctx.controller.clone();

    let theme = ctx.cx.theme();
    let colors = &theme.colors;
    let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
    let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
    let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
    let shadow = theme.elevation(Elevation::Floating);
    let open = info.tab_menu_open;
    let show_badge = info.show_tab_badge;
    let icons_in_tabs = info.icons_in_tabs;

    let chevron = IconButton::new(
        "host-tab-overflow",
        if open {
            IconName::Minus
        } else {
            IconName::ChevronDown
        },
    )
    .tooltip("All tabs")
    .on_click({
        let ctrl = controller.clone();
        move |_, _w, cx| {
            ctrl.update(cx, |c, cx| c.dispatch(Cmd::ToggleTabMenu, cx));
        }
    });

    let rows = info.tabs.iter().enumerate().map(|(ix, tab)| {
        let (id, app, exe, icon) = (tab.id, &tab.app, &tab.exe, &tab.icon);
        let active = info.active.as_ref() == Some(&id);
        let ctrl = controller.clone();
        let tab_id = id;
        div()
            .id(("tab-overflow-row", ix))
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .px_2()
            .py_1()
            .rounded(rad_sm)
            .text_size(text_sm)
            .text_color(if active { colors.fg } else { colors.fg_muted })
            .cursor_pointer()
            .hover(|style| style.bg(colors.hover))
            .on_click(move |_, _w, cx| {
                ctrl.update(cx, |c, cx| {
                    if let Some(g) = group {
                        c.dispatch(
                            Cmd::SelectTab {
                                group: g,
                                tab: tab_id,
                            },
                            cx,
                        );
                    }
                    c.dispatch(Cmd::SetTabMenu(false), cx);
                });
            })
            .map(|el| match (icons_in_tabs, icon) {
                (true, Some(image)) => el.child(
                    Image::new(("tab-overflow-icon", ix), image.clone())
                        .flex_none()
                        .w(icon_px)
                        .h(icon_px),
                ),
                _ => el.child(
                    Icon::new(IconName::AppWindow)
                        .size(IconSize::Sm)
                        .color(colors.fg_subtle),
                ),
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_color(if active { colors.fg } else { colors.fg_muted })
                            .child(Ellipsis::new(app.clone())),
                    )
                    .child(
                        div()
                            .text_size(text_xs)
                            .text_color(colors.fg_subtle)
                            .child(Ellipsis::new(exe.clone())),
                    ),
            )
    });

    let empty_row = info.tabs.is_empty().then(|| {
        div()
            .px_2()
            .py_1()
            .text_size(text_sm)
            .text_color(colors.fg_subtle)
            .child("No windows yet")
    });

    let panel = div()
        .flex()
        .flex_col()
        .gap_0p5()
        .p_1()
        .w(px(300.0))
        .rounded(rad_lg)
        .bg(colors.overlay)
        .border_1()
        .border_color(colors.border)
        .shadow(shadow)
        .children(rows)
        .children(empty_row);

    let wrap = div()
        .id("tab-overflow-wrap")
        .relative()
        .flex_none()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
        .child({
            let n = info.tabs.len();
            if show_badge && n > 0 {
                let label = if n > 99 {
                    "99+".to_string()
                } else {
                    n.to_string()
                };
                let pill = div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .min_w(px(11.0))
                    .h(px(11.0))
                    .px(px(2.0))
                    .rounded_full()
                    .bg(colors.accent)
                    .text_color(colors.on_accent)
                    .text_size(px(8.5))
                    .line_height(px(8.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(label);
                div()
                    .relative()
                    .flex_none()
                    .child(chevron)
                    .child(div().absolute().top(px(-3.0)).right(px(-3.0)).child(pill))
                    .into_any_element()
            } else {
                chevron.into_any_element()
            }
        })
        .child({
            let anchor = anchor.clone();
            canvas(
                move |bounds, window, cx| {
                    let changed = anchor.update(cx, |a, _| std::mem::replace(a, bounds) != bounds);
                    if changed {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        })
        .into_any_element();

    let overlay = open.then(|| {
        let at = anchor.read(ctx.cx).bottom_left();
        overlay_layer(
            "host-tab-overflow",
            at,
            300.0,
            viewport,
            enter_animation(ctx.cx),
            panel,
        )
    });
    (wrap, overlay)
}

pub(super) fn picker(ctx: &mut HostCtx) -> (AnyElement, Option<AnyElement>) {
    let viewport = ctx.viewport;
    let anchor = ctx
        .window
        .use_keyed_state(("host-picker-anchor", 0usize), ctx.cx, |_, _| {
            Bounds::default()
        });
    let info = ctx.info;
    let controller = ctx.controller.clone();

    let wrap = div()
        .id("picker-wrap")
        .relative()
        .flex_none()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
        .child(
            IconButton::new("host-add-window", IconName::Plus)
                .tooltip("Add a window")
                .on_click({
                    let ctrl = controller.clone();
                    move |_, _w, cx| {
                        ctrl.update(cx, |c, cx| c.dispatch(Cmd::TogglePicker, cx));
                    }
                }),
        )
        .child({
            let anchor = anchor.clone();
            canvas(
                move |bounds, window, cx| {
                    let changed = anchor.update(cx, |a, _| std::mem::replace(a, bounds) != bounds);
                    if changed {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        });

    let overlay = info.picker_open.then(|| {
        let theme = ctx.cx.theme();
        let colors = &theme.colors;
        let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
        let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
        let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
        let shadow = theme.elevation(Elevation::Floating);

        let mut rows: Vec<AnyElement> = Vec::with_capacity(info.picker_rows.len() + 1);
        rows.push(
            div()
                .id("picker-pick-row")
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .px_2()
                .py_1()
                .rounded(rad_sm)
                .text_size(text_sm)
                .text_color(colors.fg)
                .cursor_pointer()
                .hover(|style| style.bg(colors.hover))
                .on_click({
                    let ctrl = controller.clone();
                    move |_, _w, cx| {
                        ctrl.update(cx, |c, cx| {
                            c.dispatch(Cmd::BeginPick, cx);
                            c.dispatch(Cmd::SetPickerOpen(false), cx);
                        });
                    }
                })
                .child(
                    Icon::new(IconName::Plus)
                        .size(IconSize::Sm)
                        .color(colors.fg_subtle),
                )
                .child(div().flex_1().min_w_0().child("Pick a window with"))
                .child(Kbd::new("tab"))
                .into_any_element(),
        );

        for (ix, row) in info.picker_rows.iter().enumerate() {
            let blocked = row.reason.is_some();
            let mut el = div()
                .id(("picker-row", ix))
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .px_2()
                .py_1()
                .rounded(rad_sm)
                .text_size(text_sm)
                .text_color(if blocked { colors.fg_subtle } else { colors.fg })
                .when(blocked, |el| el.opacity(0.5));
            if !blocked {
                el = el.cursor_pointer().hover(|style| style.bg(colors.hover));
            }
            el = el.on_click({
                let ctrl = controller.clone();
                let hwnd = row.hwnd;
                move |_, _w, cx| {
                    if blocked {
                        return;
                    }
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::AddWindow { hwnd }, cx);
                        c.dispatch(Cmd::SetPickerOpen(false), cx);
                    });
                }
            });
            el = match &row.icon {
                Some(image) => el.child(
                    Image::new(("picker-icon", ix), image.clone())
                        .flex_none()
                        .w(icon_px)
                        .h(icon_px),
                ),
                None => el.child(
                    Icon::new(IconName::AppWindow)
                        .size(IconSize::Sm)
                        .color(colors.fg_subtle),
                ),
            };
            let detail = row.reason.clone().unwrap_or_else(|| row.exe.clone());
            el = el.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_color(if blocked { colors.fg_subtle } else { colors.fg })
                            .child(Ellipsis::new(row.title.clone())),
                    )
                    .child(
                        div()
                            .text_size(text_xs)
                            .text_color(colors.fg_subtle)
                            .child(Ellipsis::new(detail)),
                    ),
            );
            rows.push(el.into_any_element());
        }

        let panel = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .p_1()
            .w(px(300.0))
            .rounded(rad_lg)
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.border)
            .shadow(shadow)
            .children(rows);

        let at = anchor.read(ctx.cx).bottom_left();
        overlay_layer(
            "host-picker",
            at,
            300.0,
            viewport,
            enter_animation(ctx.cx),
            panel,
        )
    });
    (wrap.into_any_element(), overlay)
}

pub(super) fn saved(ctx: &mut HostCtx) -> (AnyElement, Option<AnyElement>) {
    let viewport = ctx.viewport;
    let anchor = ctx
        .window
        .use_keyed_state(("host-saved-anchor", 0usize), ctx.cx, |_, _| {
            Bounds::default()
        });
    let info = ctx.info;
    let controller = ctx.controller.clone();

    let wrap = div()
        .id("saved-wrap")
        .relative()
        .flex_none()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
        .child(
            IconButton::new("host-saved-sets", IconName::Layers)
                .tooltip("Saved tab sets")
                .on_click({
                    let ctrl = controller.clone();
                    move |_, _w, cx| {
                        ctrl.update(cx, |c, cx| c.dispatch(Cmd::ToggleSavedMenu, cx));
                    }
                }),
        )
        .child({
            let anchor = anchor.clone();
            canvas(
                move |bounds, window, cx| {
                    let changed = anchor.update(cx, |a, _| std::mem::replace(a, bounds) != bounds);
                    if changed {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        });

    let overlay = info.saved_menu_open.then(|| {
        let theme = ctx.cx.theme();
        let colors = &theme.colors;
        let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
        let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
        let shadow = theme.elevation(Elevation::Floating);
        let save_enabled = !info.tabs.is_empty();

        let mut rows: Vec<AnyElement> = Vec::with_capacity(info.saved_sets.len() + 2);

        rows.push(
            div()
                .id("saved-save-row")
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .px_2()
                .py_1()
                .rounded(rad_sm)
                .text_size(text_sm)
                .text_color(if save_enabled {
                    colors.fg
                } else {
                    colors.fg_subtle
                })
                .when(save_enabled, |el| {
                    el.cursor_pointer()
                        .hover(|style| style.bg(colors.hover))
                        .on_click({
                            let ctrl = controller.clone();
                            move |_, _w, cx| {
                                ctrl.update(cx, |c, cx| c.dispatch(Cmd::BeginSaveSet, cx));
                            }
                        })
                })
                .when(!save_enabled, |el| el.opacity(0.5))
                .child(
                    Icon::new(IconName::Save)
                        .size(IconSize::Sm)
                        .color(colors.fg_subtle),
                )
                .child(div().flex_1().min_w_0().child("Save current tabs\u{2026}"))
                .into_any_element(),
        );

        for set in info.saved_sets.iter() {
            let id = set.id;
            let restore = {
                let ctrl = controller.clone();
                move |_: &gpui::ClickEvent, _w: &mut Window, cx: &mut gpui::App| {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::RestoreSet { id }, cx);
                        c.dispatch(Cmd::SetSavedMenu(false), cx);
                    });
                }
            };
            let rename = {
                let ctrl = controller.clone();
                move |_: &gpui::ClickEvent, _w: &mut Window, cx: &mut gpui::App| {
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::BeginRenameSet { id }, cx));
                }
            };
            let delete = {
                let ctrl = controller.clone();
                move |_: &gpui::ClickEvent, _w: &mut Window, cx: &mut gpui::App| {
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::DeleteSet { id }, cx));
                }
            };
            let closed = set.count.saturating_sub(set.available);
            let detail = if closed == 0 {
                format!("{} available", set.available)
            } else if set.available == 0 {
                format!("{closed} closed")
            } else {
                format!("{} available \u{00b7} {closed} closed", set.available)
            };
            let detail_color = if set.available == 0 {
                colors.fg_subtle
            } else {
                colors.fg_muted
            };
            rows.push(
                div()
                    .id(("saved-row", id))
                    .flex()
                    .items_center()
                    .gap_1()
                    .w_full()
                    .rounded(rad_sm)
                    .hover(|style| style.bg(colors.hover))
                    .child(
                        div()
                            .id(("saved-restore", id))
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .on_click(restore)
                            .child(
                                Icon::new(IconName::FolderOpen)
                                    .size(IconSize::Sm)
                                    .color(colors.fg_subtle),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(text_sm)
                                    .text_color(if set.available == 0 {
                                        colors.fg_muted
                                    } else {
                                        colors.fg
                                    })
                                    .child(Ellipsis::new(set.name.clone()))
                                    .child(
                                        div()
                                            .text_size(text_xs)
                                            .text_color(detail_color)
                                            .child(detail),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_0p5()
                            .px_1()
                            .occlude()
                            .child(
                                div()
                                    .id(("saved-rename", id))
                                    .p_1()
                                    .rounded(rad_sm)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(colors.hover))
                                    .on_click(rename)
                                    .child(
                                        Icon::new(IconName::Pencil)
                                            .size(IconSize::Sm)
                                            .color(colors.fg_muted),
                                    ),
                            )
                            .child(
                                div()
                                    .id(("saved-delete", id))
                                    .p_1()
                                    .rounded(rad_sm)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(colors.hover))
                                    .on_click(delete)
                                    .child(
                                        Icon::new(IconName::Trash2)
                                            .size(IconSize::Sm)
                                            .color(colors.danger),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
        }

        if info.saved_sets.is_empty() {
            rows.push(
                div()
                    .px_2()
                    .py_1()
                    .text_size(text_xs)
                    .text_color(colors.fg_subtle)
                    .child("No saved sets yet")
                    .into_any_element(),
            );
        }

        let panel = div()
            .flex()
            .flex_col()
            .gap_0p5()
            .p_1()
            .w(px(280.0))
            .rounded(rad_lg)
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.border)
            .shadow(shadow)
            .children(rows);

        let at = anchor.read(ctx.cx).bottom_left();
        overlay_layer(
            "host-saved",
            at,
            280.0,
            viewport,
            enter_animation(ctx.cx),
            panel,
        )
    });
    (wrap.into_any_element(), overlay)
}

pub(super) fn tab_menu(ctx: &mut HostCtx) -> Option<(Point<gpui::Pixels>, gpui::Div)> {
    let info = ctx.info;
    let controller = ctx.controller.clone();
    let (group, tab, at) = {
        let menu = info.tab_menu.as_ref()?;
        let group = ctx.group?;
        (group, menu.tab, menu.at)
    };

    let theme = ctx.cx.theme();
    let colors = &theme.colors;
    let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
    let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
    let row_h = theme.control_height(ControlSize::Md);
    let shadow = theme.elevation(Elevation::Floating);
    let ix = info.tabs.iter().position(|t| t.id == tab);
    let can_right = ix.is_some_and(|ix| ix + 1 < info.tabs.len());
    let can_left = ix.is_some_and(|ix| ix > 0);
    let tab_count = info.tabs.len();

    let item =
        |id: &'static str,
         label: &'static str,
         desc: Option<&'static str>,
         icon: IconName,
         danger: bool,
         enabled: bool,
         run: Action| {
            let text_color = if danger {
                colors.danger
            } else {
                colors.fg_muted
            };
            let icon_color = if danger {
                colors.danger
            } else {
                colors.fg_subtle
            };
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .px_2()
                .min_h(row_h)
                .when(desc.is_some(), |el| el.py_1())
                .rounded(rad_sm)
                .text_size(text_sm)
                .text_color(text_color)
                .when(!enabled, |el| el.opacity(0.4))
                .when(enabled, |el| {
                    el.cursor_pointer().hover(|style| style.bg(colors.hover))
                })
                .on_click(move |_, window, cx| {
                    if enabled {
                        run(window, cx);
                    }
                })
                .child(Icon::new(icon).size(IconSize::Sm).color(icon_color))
                .child(div().flex_col().flex_1().min_w_0().child(label).when_some(
                    desc,
                    |el, text| {
                        el.child(
                            div()
                                .text_size(text_xs)
                                .text_color(colors.fg_subtle)
                                .child(text),
                        )
                    },
                ))
        };

    let ungroup = {
        let ctrl = controller.clone();
        Rc::new(move |_w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::Ungroup { group, tab }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }) as Action
    };
    let close = {
        let ctrl = controller.clone();
        Rc::new(move |_w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::CloseTab { group, tab }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }) as Action
    };
    let others = {
        let ctrl = controller.clone();
        Rc::new(move |_w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::CloseOtherTabs { group, tab }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }) as Action
    };
    let left = {
        let ctrl = controller.clone();
        Rc::new(move |_w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::CloseTabsToLeft { group, tab }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }) as Action
    };
    let right = {
        let ctrl = controller.clone();
        Rc::new(move |_w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::CloseTabsToRight { group, tab }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }) as Action
    };
    let rename = {
        let ctrl = controller.clone();
        Rc::new(move |_w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::BeginRename { group, tab }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }) as Action
    };
    let exe_path = ix
        .and_then(|ix| info.tabs.get(ix))
        .map(|t| t.exe_path.clone())
        .unwrap_or_default();
    let copy_path = {
        let path = exe_path.clone();
        let ctrl = controller.clone();
        Rc::new(move |_w: &mut Window, cx: &mut gpui::App| {
            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
            ctrl.update(cx, |c, cx| c.dispatch(Cmd::CloseTabMenu, cx));
        }) as Action
    };

    let divider = |id: &'static str| div().id(id).my_1().h(px(1.0)).w_full().bg(colors.border);

    let accent_now = ix.and_then(|i| info.tabs.get(i)).and_then(|t| t.accent);
    let set_color = |color: Option<usize>| {
        let ctrl = controller.clone();
        move |_: &gpui::ClickEvent, _w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::SetTabColor { group, tab, color }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }
    };
    let none_seg = {
        let selected = accent_now.is_none();
        let click = set_color(None);
        div()
            .id("ctx-swatch-none")
            .flex_1()
            .h(px(4.0))
            .rounded_full()
            .border_1()
            .border_color(colors.border_strong)
            .cursor_pointer()
            .when(selected, |el| el.h(px(7.0)).bg(colors.hover))
            .hover(|style| style.opacity(0.8))
            .on_click(click)
    };
    let hues: Vec<_> = HUE_NAMES
        .iter()
        .enumerate()
        .map(|(index, name)| ctx.palette.hue(index, name))
        .collect();
    let swatches = HUE_NAMES.iter().enumerate().map(|(index, _name)| {
        let selected = accent_now == Some(index);
        let color = hues[index];
        let click = set_color(Some(index));
        div()
            .id(("ctx-swatch", index))
            .flex_1()
            .h(px(4.0))
            .rounded_full()
            .bg(color)
            .cursor_pointer()
            .when(selected, |el| el.h(px(7.0)))
            .hover(|style| style.opacity(0.8))
            .on_click(click)
    });
    let color_row = div()
        .id("ctx-color")
        .flex()
        .items_center()
        .gap_1()
        .w_full()
        .px_1()
        .py_2()
        .child(none_seg)
        .children(swatches);

    let rows = vec![
        color_row,
        divider("ctx-divider-color"),
        item(
            "ctx-rename",
            "Rename\u{2026}",
            None,
            IconName::Pencil,
            false,
            true,
            rename,
        ),
        item(
            "ctx-copy-path",
            "Copy exe path",
            None,
            IconName::Copy,
            false,
            !exe_path.is_empty(),
            copy_path,
        ),
        divider("ctx-divider-a"),
        item(
            "ctx-others",
            "Close others",
            None,
            IconName::AppWindow,
            false,
            tab_count > 1,
            others,
        ),
        item(
            "ctx-left",
            "Ungroup to the left",
            None,
            IconName::ChevronLeft,
            false,
            can_left,
            left,
        ),
        item(
            "ctx-right",
            "Ungroup to the right",
            None,
            IconName::ChevronRight,
            false,
            can_right,
            right,
        ),
        divider("ctx-divider-b"),
        item(
            "ctx-ungroup",
            "Ungroup",
            Some("Un-host the window and keep it open"),
            IconName::ExternalLink,
            false,
            true,
            ungroup,
        ),
        item(
            "ctx-close",
            "Close",
            Some("Close the window and end its app"),
            IconName::X,
            true,
            true,
            close,
        ),
    ];

    let panel = div()
        .flex()
        .flex_col()
        .gap_0p5()
        .p_1()
        .w(px(260.0))
        .rounded(rad_lg)
        .bg(colors.overlay)
        .border_1()
        .border_color(colors.border)
        .shadow(shadow)
        .children(rows);

    Some((gpui::point(px(at.0), px(at.1)), panel))
}
