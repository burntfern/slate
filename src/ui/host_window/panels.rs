use std::rc::Rc;

use ely_gpui_component::buttons::IconButton;
use ely_gpui_component::primitives::{Icon, IconName, Image};
use ely_gpui_component::theme::{ActiveTheme, ControlSize, IconSize, Radius, TextSize, HUE_NAMES};
use ely_gpui_component::typography::{Ellipsis, Kbd};
use gpui::prelude::*;
use gpui::{
    canvas, div, px, AnyElement, App, Bounds, ClipboardItem, Div, Entity, FontWeight, MouseButton,
    Point, Role, Styled, Window,
};

use super::overlays::{
    enter_animation, menu_height, menu_panel, overlay_layer, Action, MENU_W, MENU_W_SAVED,
    MENU_W_TAB_MENU,
};
use super::HostCtx;
use crate::controller::{Cmd, Controller, PickerRow, SavedSetInfo, TabInfo};
use crate::core::{GroupId, TabId};

const SWATCH_H: f32 = 4.0;
const SWATCH_H_ACTIVE: f32 = 7.0;
const BADGE_SIZE: f32 = 11.0;
const BADGE_PAD_X: f32 = 2.0;
const BADGE_FONT: f32 = 8.5;
const BADGE_OFFSET: f32 = -3.0;

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
    let open = info.tab_menu_open;
    let show_badge = info.show_tab_badge;
    let icons_in_tabs = info.icons_in_tabs;

    let theme = ctx.cx.theme();
    let colors = &theme.colors;
    let text_sm = theme.text_size(TextSize::Sm);

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
        overflow_row(
            ix,
            tab,
            info.active.as_ref() == Some(&tab.id),
            icons_in_tabs,
            group,
            &controller,
            ctx.cx,
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

    let panel = menu_panel(MENU_W, ctx.cx)
        .children(rows)
        .children(empty_row);

    let trigger = if show_badge && !info.tabs.is_empty() {
        div()
            .relative()
            .flex_none()
            .child(chevron)
            .child(
                div()
                    .absolute()
                    .top(px(BADGE_OFFSET))
                    .right(px(BADGE_OFFSET))
                    .child(tab_badge(info.tabs.len(), ctx.cx)),
            )
            .into_any_element()
    } else {
        chevron.into_any_element()
    };

    let wrap = div()
        .id("tab-overflow-wrap")
        .relative()
        .flex_none()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
        .child(trigger)
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
            MENU_W,
            menu_height(info.tabs.len().max(1)),
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
        let mut rows: Vec<AnyElement> = Vec::with_capacity(info.picker_rows.len() + 1);
        rows.push(picker_pick_row(&controller, ctx.cx));
        for (ix, row) in info.picker_rows.iter().enumerate() {
            rows.push(picker_row(ix, row, &controller, ctx.cx));
        }

        let panel = menu_panel(MENU_W, ctx.cx).children(rows);

        let at = anchor.read(ctx.cx).bottom_left();
        overlay_layer(
            "host-picker",
            at,
            MENU_W,
            menu_height(info.picker_rows.len().max(1)),
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
        let save_enabled = !info.tabs.is_empty();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(info.saved_sets.len() + 2);

        rows.push(saved_save_row(save_enabled, &controller, ctx.cx));
        for set in info.saved_sets.iter() {
            rows.push(saved_set_row(set, &controller, ctx.cx));
        }

        if info.saved_sets.is_empty() {
            let theme = ctx.cx.theme();
            rows.push(
                div()
                    .px_2()
                    .py_1()
                    .text_size(theme.text_size(TextSize::Xs))
                    .text_color(theme.colors.fg_subtle)
                    .child("No saved sets yet")
                    .into_any_element(),
            );
        }

        let panel = menu_panel(MENU_W_SAVED, ctx.cx).children(rows);

        let at = anchor.read(ctx.cx).bottom_left();
        overlay_layer(
            "host-saved",
            at,
            MENU_W_SAVED,
            menu_height(info.saved_sets.len().max(1)),
            viewport,
            enter_animation(ctx.cx),
            panel,
        )
    });
    (wrap.into_any_element(), overlay)
}

pub(super) fn tab_menu(ctx: &mut HostCtx) -> Option<(Point<gpui::Pixels>, gpui::Div, f32)> {
    let info = ctx.info;
    let controller = ctx.controller.clone();
    let (group, tab, at) = {
        let menu = info.tab_menu.as_ref()?;
        let group = ctx.group?;
        (group, menu.tab, menu.at)
    };

    let ix = info.tabs.iter().position(|t| t.id == tab);
    let can_right = ix.is_some_and(|ix| ix + 1 < info.tabs.len());
    let can_left = ix.is_some_and(|ix| ix > 0);
    let tab_count = info.tabs.len();

    let ungroup = close_menu_action(&controller, group, tab, |group, tab| Cmd::Ungroup {
        group,
        tab,
    });
    let close = close_menu_action(&controller, group, tab, |group, tab| Cmd::CloseTab {
        group,
        tab,
    });
    let others = close_menu_action(&controller, group, tab, |group, tab| Cmd::CloseOtherTabs {
        group,
        tab,
    });
    let left = close_menu_action(&controller, group, tab, |group, tab| Cmd::CloseTabsToLeft {
        group,
        tab,
    });
    let right = close_menu_action(&controller, group, tab, |group, tab| {
        Cmd::CloseTabsToRight { group, tab }
    });
    let rename = close_menu_action(&controller, group, tab, |group, tab| Cmd::BeginRename {
        group,
        tab,
    });
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

    let accent_now = ix.and_then(|i| info.tabs.get(i)).and_then(|t| t.accent);

    let rows = vec![
        accent_row(accent_now, group, tab, &controller, &ctx.palette, ctx.cx),
        menu_divider("ctx-divider-color", ctx.cx),
        menu_item(
            MenuItem::new(
                "ctx-rename",
                "Rename\u{2026}",
                IconName::Pencil,
                true,
                rename,
            ),
            ctx.cx,
        ),
        menu_item(
            MenuItem::new(
                "ctx-copy-path",
                "Copy exe path",
                IconName::Copy,
                !exe_path.is_empty(),
                copy_path,
            ),
            ctx.cx,
        ),
        menu_divider("ctx-divider-a", ctx.cx),
        menu_item(
            MenuItem::new(
                "ctx-others",
                "Close others",
                IconName::AppWindow,
                tab_count > 1,
                others,
            ),
            ctx.cx,
        ),
        menu_item(
            MenuItem::new(
                "ctx-left",
                "Ungroup to the left",
                IconName::ChevronLeft,
                can_left,
                left,
            ),
            ctx.cx,
        ),
        menu_item(
            MenuItem::new(
                "ctx-right",
                "Ungroup to the right",
                IconName::ChevronRight,
                can_right,
                right,
            ),
            ctx.cx,
        ),
        menu_divider("ctx-divider-b", ctx.cx),
        menu_item(
            MenuItem::new(
                "ctx-ungroup",
                "Ungroup",
                IconName::ExternalLink,
                true,
                ungroup,
            )
            .desc("Un-host the window and keep it open"),
            ctx.cx,
        ),
        menu_item(
            MenuItem::new("ctx-close", "Close", IconName::X, true, close)
                .desc("Close the window and end its app")
                .danger(),
            ctx.cx,
        ),
    ];

    let rows_len = rows.len();

    let panel = menu_panel(MENU_W_TAB_MENU, ctx.cx).children(rows);

    Some((
        gpui::point(px(at.0), px(at.1)),
        panel,
        menu_height(rows_len),
    ))
}

fn tab_badge(count: usize, cx: &App) -> Div {
    let theme = cx.theme();
    let colors = &theme.colors;
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    div()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(BADGE_SIZE))
        .h(px(BADGE_SIZE))
        .px(px(BADGE_PAD_X))
        .rounded_full()
        .bg(colors.accent)
        .text_color(colors.on_accent)
        .text_size(px(BADGE_FONT))
        .line_height(px(BADGE_FONT))
        .font_weight(FontWeight::MEDIUM)
        .child(label)
}

fn overflow_row(
    ix: usize,
    tab: &TabInfo,
    active: bool,
    icons_in_tabs: bool,
    group: Option<GroupId>,
    controller: &Entity<Controller>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
    let rad_sm = theme.radius(Radius::Sm);
    let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
    let (app, exe, icon) = (&tab.app, &tab.exe, &tab.icon);
    let tab_id = tab.id;
    let ctrl = controller.clone();
    div()
        .id(("tab-overflow-row", ix))
        .role(Role::MenuItem)
        .aria_label(app.clone())
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
        .into_any_element()
}

fn picker_pick_row(controller: &Entity<Controller>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let rad_sm = theme.radius(Radius::Sm);
    let text_sm = theme.text_size(TextSize::Sm);
    let ctrl = controller.clone();
    div()
        .id("picker-pick-row")
        .role(Role::MenuItem)
        .aria_label("Pick a window with Tab")
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
        .on_click(move |_, _w, cx| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::BeginPick, cx);
                c.dispatch(Cmd::SetPickerOpen(false), cx);
            });
        })
        .child(
            Icon::new(IconName::Plus)
                .size(IconSize::Sm)
                .color(colors.fg_subtle),
        )
        .child(div().flex_1().min_w_0().child("Pick a window with"))
        .child(Kbd::new("tab"))
        .into_any_element()
}

fn picker_row(ix: usize, row: &PickerRow, controller: &Entity<Controller>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
    let rad_sm = theme.radius(Radius::Sm);
    let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
    let blocked = row.reason.is_some();
    let mut el = div()
        .id(("picker-row", ix))
        .role(Role::MenuItem)
        .aria_label(row.title.clone())
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
    el.child(
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
    )
    .into_any_element()
}

fn saved_save_row(save_enabled: bool, controller: &Entity<Controller>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let rad_sm = theme.radius(Radius::Sm);
    let text_sm = theme.text_size(TextSize::Sm);
    div()
        .id("saved-save-row")
        .role(Role::MenuItem)
        .aria_label("Save current tabs")
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
        .into_any_element()
}

fn saved_set_row(set: &SavedSetInfo, controller: &Entity<Controller>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let rad_sm = theme.radius(Radius::Sm);
    let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
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
    let detail_color = if set.available == 0 {
        colors.fg_subtle
    } else {
        colors.fg_muted
    };
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
                .role(Role::MenuItem)
                .aria_label(set.name.clone())
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
                                .child(format!(
                                    "{} windows - {} available",
                                    set.total, set.available
                                )),
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
        .into_any_element()
}

fn menu_divider(id: &'static str, cx: &App) -> AnyElement {
    div()
        .id(id)
        .my_1()
        .h(px(1.0))
        .w_full()
        .bg(cx.theme().colors.border)
        .into_any_element()
}

struct MenuItem {
    id: &'static str,
    label: &'static str,
    desc: Option<&'static str>,
    icon: IconName,
    danger: bool,
    enabled: bool,
    run: Action,
}

impl MenuItem {
    fn new(
        id: &'static str,
        label: &'static str,
        icon: IconName,
        enabled: bool,
        run: Action,
    ) -> Self {
        Self {
            id,
            label,
            desc: None,
            icon,
            danger: false,
            enabled,
            run,
        }
    }

    fn desc(mut self, desc: &'static str) -> Self {
        self.desc = Some(desc);
        self
    }

    fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

fn menu_item(item: MenuItem, cx: &App) -> AnyElement {
    let MenuItem {
        id,
        label,
        desc,
        icon,
        danger,
        enabled,
        run,
    } = item;
    let theme = cx.theme();
    let colors = &theme.colors;
    let rad_sm = theme.radius(Radius::Sm);
    let (text_sm, text_xs) = (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
    let row_h = theme.control_height(ControlSize::Md);
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
        .role(Role::MenuItem)
        .aria_label(label)
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
        .child(
            div()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(label)
                .when_some(desc, |el, text| {
                    el.child(
                        div()
                            .text_size(text_xs)
                            .text_color(colors.fg_subtle)
                            .child(text),
                    )
                }),
        )
        .into_any_element()
}

fn accent_row(
    accent_now: Option<usize>,
    group: GroupId,
    tab: TabId,
    controller: &Entity<Controller>,
    palette: &ely_gpui_component::theme::Palette,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let colors = &theme.colors;
    let set_color = |color: Option<usize>| {
        let ctrl = controller.clone();
        move |_: &gpui::ClickEvent, _w: &mut Window, cx: &mut gpui::App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::SetTabColor { group, tab, color }, cx);
                c.dispatch(Cmd::CloseTabMenu, cx);
            });
        }
    };
    let selected = accent_now.is_none();
    let click = set_color(None);
    let none_seg = div()
        .id("ctx-swatch-none")
        .role(Role::Button)
        .aria_label("No accent color")
        .flex_1()
        .h(px(SWATCH_H))
        .rounded_full()
        .border_1()
        .border_color(colors.border_strong)
        .cursor_pointer()
        .when(selected, |el| el.h(px(SWATCH_H_ACTIVE)).bg(colors.hover))
        .hover(|style| style.opacity(0.8))
        .on_click(click);
    let hues: Vec<_> = HUE_NAMES
        .iter()
        .enumerate()
        .map(|(index, name)| palette.hue(index, name))
        .collect();
    let swatches = HUE_NAMES.iter().enumerate().map(|(index, name)| {
        let selected = accent_now == Some(index);
        let color = hues[index];
        let click = set_color(Some(index));
        div()
            .id(("ctx-swatch", index))
            .role(Role::Button)
            .aria_label(*name)
            .flex_1()
            .h(px(SWATCH_H))
            .rounded_full()
            .bg(color)
            .cursor_pointer()
            .when(selected, |el| el.h(px(SWATCH_H_ACTIVE)))
            .hover(|style| style.opacity(0.8))
            .on_click(click)
    });
    div()
        .id("ctx-color")
        .flex()
        .items_center()
        .gap_1()
        .w_full()
        .px_1()
        .py_2()
        .child(none_seg)
        .children(swatches)
        .into_any_element()
}

fn close_menu_action(
    controller: &Entity<Controller>,
    group: GroupId,
    tab: TabId,
    make: impl Fn(GroupId, TabId) -> Cmd + 'static,
) -> Action {
    let ctrl = controller.clone();
    Rc::new(move |_w: &mut Window, cx: &mut App| {
        ctrl.update(cx, |c, cx| {
            c.dispatch(make(group, tab), cx);
            c.dispatch(Cmd::CloseTabMenu, cx);
        });
    }) as Action
}
