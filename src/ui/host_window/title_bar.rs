use ely_gpui_component::buttons::IconButton;
use ely_gpui_component::menus::{Menu, MenuItem, OverflowMenu};
use ely_gpui_component::motion;
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::shell::{drag_region, WindowControls};
use ely_gpui_component::theme::{ActiveTheme, ControlSize, IconSize, Mode};
use gpui::prelude::*;
use gpui::{
    canvas, div, linear_color_stop, linear_gradient, px, AnyElement, App, Div, Entity,
    ScrollHandle, ScrollWheelEvent, Styled, Window,
};

use super::overlays::{enter_animation, overlay_layer, MENU_W_TAB_MENU};
use super::panels;
use super::HostCtx;
use crate::controller::{Cmd, Controller, HostInfo};
use crate::core::update::UpdateState;
use crate::ui::tab_bar::SlateTabBar;

/// Width of each scroll fade over the tab strip.
const FADE_W: f32 = 40.0;
/// Update-available dot on the menu button.
const UPDATE_DOT_SIZE: f32 = 8.0;
const UPDATE_DOT_INSET: f32 = 4.0;
/// Space reserved before the OS window controls.
const CONTROLS_RESERVE_W: f32 = 64.0;

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

    let menu_wrap = menu_trigger(&controller, info, ctx.palette.success);

    let (tab_overflow, chevron_overlay) = panels::tab_overflow(ctx);
    let (picker_wrap, picker_overlay) = panels::picker(ctx);
    let (saved_wrap, saved_overlay) = panels::saved(ctx);

    let pick_slot: AnyElement = if info.picking {
        picking_spinner(ctx.cx)
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
        .pl(px(leading_inset(ctx.cx)))
        .child(menu_wrap)
        .child(tab_strip(info, tab_bar, tab_scroll, bg))
        .child(pick_slot)
        .child(tab_overflow)
        .child(saved_wrap)
        .child(theme_button(&controller, ctx.cx))
        .child(div().flex_none().w(px(CONTROLS_RESERVE_W)))
        .child(
            div()
                .flex_none()
                .occlude()
                .child(window_controls(&controller)),
        );

    let tab_menu_overlay = panels::tab_menu(ctx).map(|(at, panel, height)| {
        overlay_layer(
            "host-tab-menu",
            at,
            MENU_W_TAB_MENU,
            height,
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

/// An action that dispatches `make()` and then clears the mirrored overlay flag.
fn overlay_action(
    controller: &Entity<Controller>,
    make: impl Fn() -> Cmd + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let ctrl = controller.clone();
    move |_w: &mut Window, cx: &mut App| {
        ctrl.update(cx, |c, cx| {
            c.dispatch(make(), cx);
            c.dispatch(Cmd::SetOverlay(false), cx);
        });
    }
}

fn quit_action(controller: &Entity<Controller>) -> impl Fn(&mut Window, &mut App) + 'static {
    let ctrl = controller.clone();
    move |window: &mut Window, cx: &mut App| {
        ctrl.update(cx, |c, cx| c.dispatch(Cmd::Shutdown, cx));
        window.remove_window();
    }
}

fn update_state(state: &UpdateState) -> (IconName, bool, bool) {
    match state {
        UpdateState::Checking => (IconName::RefreshCw, true, false),
        UpdateState::Available(_) => (IconName::Download, false, true),
        UpdateState::UpToDate => (IconName::CircleCheck, true, false),
        UpdateState::Idle => (IconName::RefreshCw, false, false),
        UpdateState::Failed => (IconName::TriangleAlert, false, false),
    }
}

fn app_menu(controller: &Entity<Controller>, info: &HostInfo) -> Menu {
    let (update_icon, update_disabled, _) = update_state(&info.update);
    let shortcuts_menu = Menu::new()
        .item(MenuItem::new("Next tab").keys("ctrl-tab"))
        .item(MenuItem::new("Previous tab").keys("ctrl-shift-tab"))
        .item(MenuItem::new("Jump to tab 1-9").keys("ctrl-1"))
        .item(MenuItem::new("Close tab").keys("ctrl-w"))
        .separator()
        .item(MenuItem::new("Pick a window").keys("tab"))
        .item(MenuItem::new("Close menu").keys("esc"));
    Menu::new()
        .item(
            MenuItem::new("Pick a window...")
                .icon(IconName::Plus)
                .keys("Tab")
                .on_click(overlay_action(controller, || Cmd::BeginPick)),
        )
        .item(
            MenuItem::new("Refresh window list")
                .on_click(overlay_action(controller, || Cmd::Refresh)),
        )
        .item(MenuItem::submenu("Keyboard shortcuts", shortcuts_menu).icon(IconName::Keyboard))
        .separator()
        .item(
            MenuItem::check("Show window icons in tabs", info.icons_in_tabs)
                .on_click(overlay_action(controller, || Cmd::ToggleIconsInTabs)),
        )
        .item(
            MenuItem::check("Show tab count badge", info.show_tab_badge)
                .on_click(overlay_action(controller, || Cmd::ToggleTabBadge)),
        )
        .item(
            MenuItem::check("Show hover preview", info.hover_preview_enabled)
                .on_click(overlay_action(controller, || Cmd::ToggleHoverPreview)),
        )
        .separator()
        .item(
            MenuItem::check("Hide tab bar", !info.show_tab_bar)
                .on_click(overlay_action(controller, || Cmd::ToggleTabBar)),
        )
        .separator()
        .item(
            MenuItem::new(info.update_label.clone())
                .icon(update_icon)
                .disabled(update_disabled)
                .on_click(overlay_action(controller, || Cmd::CheckForUpdates)),
        )
        .item(
            MenuItem::new("Open GitHub repository")
                .icon(IconName::ExternalLink)
                .on_click(overlay_action(controller, || Cmd::OpenRepository)),
        )
        .separator()
        .item(MenuItem::new(info.version_label.clone()).disabled(true))
        .item(MenuItem::new("Quit Slate").on_click(quit_action(controller)))
}

fn menu_trigger(
    controller: &Entity<Controller>,
    info: &HostInfo,
    success: gpui::Hsla,
) -> impl IntoElement {
    let (_, _, update_available) = update_state(&info.update);
    // OverflowMenu owns its open state, so `info.overlay` mirrors it: the trigger toggles both and every menu action clears it.
    let menu = OverflowMenu::new("host-menu", app_menu(controller, info))
        .icon(if info.overlay {
            IconName::X
        } else {
            IconName::Menu
        })
        .tooltip("Slate menu");
    let ctrl = controller.clone();
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
                    .top(px(UPDATE_DOT_INSET))
                    .right(px(UPDATE_DOT_INSET))
                    .size(px(UPDATE_DOT_SIZE))
                    .rounded_full()
                    .bg(success),
            )
        })
}

fn theme_button(controller: &Entity<Controller>, cx: &App) -> impl IntoElement {
    let mode = cx.theme().mode();
    let ctrl = controller.clone();
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
}

fn window_controls(controller: &Entity<Controller>) -> WindowControls {
    WindowControls::new("host-controls").on_close({
        let ctrl = controller.clone();
        move |window, cx| {
            ctrl.update(cx, |c, cx| c.dispatch(Cmd::Shutdown, cx));
            window.remove_window();
        }
    })
}

fn leading_inset(cx: &App) -> f32 {
    let theme = cx.theme();
    let rem = theme.base_rem();
    let bar = theme.titlebar_height().to_pixels(rem).as_f32();
    let icon = theme
        .control_height(ControlSize::Md)
        .to_pixels(rem)
        .as_f32();
    ((bar - icon) / 2.0).max(0.0)
}

fn picking_spinner(cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (spinner_color, side) = (
        theme.colors.fg_muted,
        theme.control_height(ControlSize::default()),
    );
    div()
        .flex_none()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .size(side)
        .child(
            motion::Spinner::new(("host-picking", 0usize))
                .style(motion::SpinnerStyle::Ring)
                .size(IconSize::Md)
                .color(spinner_color),
        )
        .into_any_element()
}

fn tab_strip(
    info: &HostInfo,
    tab_bar: SlateTabBar,
    tab_scroll: &ScrollHandle,
    bg: gpui::Hsla,
) -> Div {
    let handle = tab_scroll.clone();
    let (offset, max) = (handle.offset().x, handle.max_offset().x);
    let overflowing = max > gpui::Pixels::ZERO;
    let clear = bg.opacity(0.0);
    let left_fade = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .w(px(FADE_W))
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
        .w(px(FADE_W))
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
                                if delta.y == gpui::Pixels::ZERO || max <= gpui::Pixels::ZERO {
                                    return;
                                }
                                let mut offset = wheel.offset();
                                let next = (offset.x + delta.y).clamp(-max, gpui::Pixels::ZERO);
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
}
