use ely_gpui_component::buttons::IconButton;
use ely_gpui_component::menus::{Menu, MenuItem, OverflowMenu};
use ely_gpui_component::motion;
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::shell::{drag_region, WindowControls};
use ely_gpui_component::theme::{ActiveTheme, ControlSize, IconSize, Mode};
use gpui::prelude::*;
use gpui::{
    canvas, div, linear_color_stop, linear_gradient, px, AnyElement, App, ScrollHandle,
    ScrollWheelEvent, Styled, Window,
};

use super::overlays::{enter_animation, overlay_layer};
use super::panels;
use super::HostCtx;
use crate::controller::Cmd;
use crate::core::update::UpdateState;
use crate::ui::tab_bar::SlateTabBar;

pub(super) struct TitleBar {
    pub element: AnyElement,
    pub chevron_overlay: Option<AnyElement>,
    pub picker_overlay: Option<AnyElement>,
    pub saved_overlay: Option<AnyElement>,
    pub tab_menu_overlay: Option<AnyElement>,
}

pub(super) fn build(
    ctx: &mut HostCtx,
    tab_bar: SlateTabBar,
    titlebar_h: gpui::Rems,
    bg: gpui::Hsla,
    tab_scroll: &ScrollHandle,
) -> TitleBar {
    let controller = ctx.controller.clone();
    let info = ctx.info;
    let viewport = ctx.viewport;

    let begin_pick = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::BeginPick, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let refresh = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::Refresh, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let toggle_icons = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::ToggleIconsInTabs, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let toggle_badge = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::ToggleTabBadge, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let toggle_hover_preview = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::ToggleHoverPreview, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let toggle_tab_bar = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::ToggleTabBar, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let quit = {
        let ctrl = controller.clone();
        move |window: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| c.dispatch(Cmd::Shutdown, cx));
            window.remove_window();
        }
    };
    let open_repo = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::OpenRepository, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let check_updates = {
        let ctrl = controller.clone();
        move |_w: &mut Window, cx: &mut App| {
            ctrl.update(cx, |c, cx| {
                c.dispatch(Cmd::CheckForUpdates, cx);
                c.dispatch(Cmd::SetOverlay(false), cx);
            });
        }
    };
    let (update_label, update_icon, update_disabled, update_available) = match &info.update {
        UpdateState::Checking => (
            "Checking for updates…".to_string(),
            IconName::RefreshCw,
            true,
            false,
        ),
        UpdateState::Available(version) => (
            format!("Update available: v{version}"),
            IconName::Download,
            false,
            true,
        ),
        UpdateState::UpToDate => (
            "Slate is up to date".to_string(),
            IconName::CircleCheck,
            true,
            false,
        ),
        UpdateState::Idle | UpdateState::Failed => (
            "Check for updates".to_string(),
            IconName::RefreshCw,
            false,
            false,
        ),
    };
    let shortcuts_menu = Menu::new()
        .item(MenuItem::new("Next tab").keys("ctrl-tab"))
        .item(MenuItem::new("Previous tab").keys("ctrl-shift-tab"))
        .item(MenuItem::new("Jump to tab 1-9").keys("ctrl-1"))
        .item(MenuItem::new("Close tab").keys("ctrl-w"))
        .separator()
        .item(MenuItem::new("Pick a window").keys("tab"))
        .item(MenuItem::new("Close menu").keys("esc"));
    let app_menu = Menu::new()
        .item(
            MenuItem::new("Pick a window…")
                .icon(IconName::Plus)
                .keys("Tab")
                .on_click(begin_pick),
        )
        .item(MenuItem::new("Refresh window list").on_click(refresh))
        .item(MenuItem::submenu("Keyboard shortcuts", shortcuts_menu).icon(IconName::Keyboard))
        .separator()
        .item(
            MenuItem::check("Show window icons in tabs", info.icons_in_tabs).on_click(toggle_icons),
        )
        .item(MenuItem::check("Show tab count badge", info.show_tab_badge).on_click(toggle_badge))
        .item(
            MenuItem::check("Show hover preview", info.hover_preview_enabled)
                .on_click(toggle_hover_preview),
        )
        .separator()
        .item(MenuItem::check("Hide tab bar", !info.show_tab_bar).on_click(toggle_tab_bar))
        .separator()
        .item(
            MenuItem::new(update_label)
                .icon(update_icon)
                .disabled(update_disabled)
                .on_click(check_updates),
        )
        .item(
            MenuItem::new("Open GitHub repository")
                .icon(IconName::ExternalLink)
                .on_click(open_repo),
        )
        .separator()
        .item(MenuItem::new(format!("Slate v{}", info.version)).disabled(true))
        .item(MenuItem::new("Quit Slate").on_click(quit));
    let menu = OverflowMenu::new("host-menu", app_menu)
        .icon(if info.overlay {
            IconName::X
        } else {
            IconName::Menu
        })
        .tooltip("Slate menu");

    let menu_wrap = {
        let ctrl = controller.clone();
        let success = ctx.palette.success;
        div()
            .id("menu-overlay-wrap")
            .relative()
            .flex_none()
            .occlude()
            .on_click(move |_ev, _w, cx| {
                cx.stop_propagation();
                ctrl.update(cx, |c, cx| c.dispatch(Cmd::ToggleOverlay, cx));
            })
            .child(menu)
            .when(update_available, |wrap| {
                wrap.child(
                    div()
                        .absolute()
                        .top(px(4.0))
                        .right(px(4.0))
                        .size(px(8.0))
                        .rounded_full()
                        .bg(success),
                )
            })
    };

    let (tab_overflow, chevron_overlay) = panels::tab_overflow(ctx);
    let (picker_wrap, picker_overlay) = panels::picker(ctx);
    let (saved_wrap, saved_overlay) = panels::saved(ctx);

    let mode = ctx.cx.theme().mode();
    let theme_ctrl = controller.clone();
    let make_theme = move || {
        let ctrl = theme_ctrl.clone();
        div().flex_none().occlude().child(
            IconButton::new(
                "host-theme",
                match mode {
                    Mode::Dark => IconName::Sun,
                    Mode::Light => IconName::Moon,
                },
            )
            .tooltip(match mode {
                Mode::Dark => "Light theme",
                Mode::Light => "Dark theme",
            })
            .on_click(move |_event, _window, cx| {
                ctrl.update(cx, |c, cx| c.dispatch(Cmd::ToggleTheme, cx));
            }),
        )
    };

    let controls = WindowControls::new("host-controls").on_close({
        let ctrl = controller.clone();
        move |window, cx| {
            ctrl.update(cx, |c, cx| c.dispatch(Cmd::Shutdown, cx));
            window.remove_window();
        }
    });

    let leading_inset = {
        let theme = ctx.cx.theme();
        let rem = theme.base_rem();
        let bar = theme.titlebar_height().to_pixels(rem).as_f32();
        let icon = theme
            .control_height(ControlSize::Md)
            .to_pixels(rem)
            .as_f32();
        ((bar - icon) / 2.0).max(0.0)
    };
    let (spinner_color, pick_side) = {
        let theme = ctx.cx.theme();
        (
            theme.colors.fg_muted,
            theme.control_height(ControlSize::default()),
        )
    };
    let pick_slot: AnyElement = if info.picking {
        div()
            .flex_none()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .size(pick_side)
            .child(
                motion::Spinner::new(("host-picking", 0usize))
                    .style(motion::SpinnerStyle::Ring)
                    .size(IconSize::Md)
                    .color(spinner_color),
            )
            .into_any_element()
    } else {
        picker_wrap
    };

    let title_bar = drag_region("host-titlebar", ctx.window, ctx.cx)
        .flex()
        .items_center()
        .gap_1()
        .flex_none()
        .h(titlebar_h)
        .bg(bg)
        .pl(px(leading_inset))
        .child(menu_wrap)
        .child({
            let handle = tab_scroll.clone();
            let (offset, max) = (handle.offset().x, handle.max_offset().x);
            let overflowing = max > gpui::Pixels::ZERO;
            let clear = bg.opacity(0.0);
            let left_fade =
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(40.0))
                    .bg(linear_gradient(
                        90.0,
                        linear_color_stop(bg, 0.0),
                        linear_color_stop(clear, 1.0),
                    ));
            let right_fade = div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(40.0))
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(clear, 0.0),
                    linear_color_stop(bg, 1.0),
                ));
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .when(info.show_tab_bar, |wrap| {
                    wrap.child(
                        div()
                            .id("host-tabs-scroll")
                            .size_full()
                            .flex()
                            .items_center()
                            .overflow_x_scroll()
                            .restrict_scroll_to_axis()
                            .track_scroll(&handle)
                            .child(tab_bar),
                    )
                    .child({
                        let wheel = handle.clone();
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _cx| {
                                let view = window.current_view();
                                let wheel = wheel.clone();
                                window.on_mouse_event(
                                    move |event: &ScrollWheelEvent, _phase, window, cx| {
                                        if !bounds.contains(&event.position) {
                                            return;
                                        }
                                        let delta = event.delta.pixel_delta(window.line_height());
                                        if delta.x != gpui::Pixels::ZERO {
                                            return;
                                        }
                                        let max = wheel.max_offset().x;
                                        if delta.y == gpui::Pixels::ZERO
                                            || max <= gpui::Pixels::ZERO
                                        {
                                            return;
                                        }
                                        let mut offset = wheel.offset();
                                        let next =
                                            (offset.x + delta.y).clamp(-max, gpui::Pixels::ZERO);
                                        if next != offset.x {
                                            offset.x = next;
                                            wheel.set_offset(offset);
                                            cx.notify(view);
                                        }
                                    },
                                );
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                    })
                    .when(overflowing && offset < gpui::Pixels::ZERO, |wrap| {
                        wrap.child(left_fade)
                    })
                    .when(overflowing && offset > -max, |wrap| wrap.child(right_fade))
                })
        })
        .child(pick_slot)
        .child(tab_overflow)
        .child(saved_wrap)
        .child(make_theme())
        .child(div().flex_none().w(px(64.0)))
        .child(div().flex_none().occlude().child(controls));

    let tab_menu_overlay = panels::tab_menu(ctx).map(|(at, panel)| {
        overlay_layer(
            "host-tab-menu",
            at,
            220.0,
            viewport,
            enter_animation(ctx.cx),
            panel,
        )
    });

    TitleBar {
        element: title_bar.into_any_element(),
        chevron_overlay,
        picker_overlay,
        saved_overlay,
        tab_menu_overlay,
    }
}
