//! US-like host window. The title bar is `[☰ menu][tabs][controls]`; the
//! TabBar's own `+` arms pick mode (hover an app, press TAB to host it). The
//! content rect below is where the active hosted member is pinned.

use std::sync::Arc;

use ely_gpui_component::{
    buttons::IconButton,
    feedback::ToastViewport,
    menus::{Menu, MenuItem, OverflowMenu},
    primitives::{raise, Icon, IconName, Image},
    shell::{drag_region, WindowControls},
    theme::{ActiveTheme, ControlSize, Elevation, IconSize, Mode, Radius, TextSize, Theme},
    typography::{Caption, Ellipsis},
};
use gpui::{
    anchored, canvas, div, point, prelude::*, px, Anchor, App, Bounds, Context, Div, Entity,
    IntoElement, MouseButton, ParentElement, Render, RenderImage, SharedString, Styled,
    Subscription, Window,
};

use crate::controller::{Cmd, Controller, TabInfo};
use crate::core::TabId;
use crate::ui::tab_bar::{SlateTab, SlateTabBar, TabIcon};

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
    /// A muted, grayscale copy of the Slate mark shown as the empty-state art.
    empty_icon: Option<Arc<RenderImage>>,
    _observe: Subscription,
    /// Re-render whenever the controller mutates (tab add/remove/retitle, live
    /// events). Without this the strip only refreshes on direct interaction.
    _observe_controller: Subscription,
}

impl HostWindow {
    pub fn new(controller: Entity<Controller>, cx: &mut Context<Self>) -> Self {
        let toaster = controller.read(cx).toaster();
        // Re-render when toasts change, so the toast viewport reflects arrivals
        // and expiries.
        let _observe = cx.observe(&toaster, |_this, _toaster, cx| cx.notify());
        let _observe_controller = cx.observe(&controller, |_this, _controller, cx| cx.notify());
        Self {
            controller,
            ready: false,
            close_routed: false,
            empty_icon: grayscale_mark(),
            _observe,
            _observe_controller,
        }
    }
}

/// Decode the selected logo (`SLATE_LOGO`) once into a desaturated image for the
/// empty state. The alpha is preserved so the mark keeps its shape; the render
/// site dials the opacity down so it reads as a watermark.
fn grayscale_mark() -> Option<Arc<RenderImage>> {
    const PNG: &[u8] = include_bytes!(env!("SLATE_LOGO"));
    let mut rgba = image::load_from_memory(PNG).ok()?.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let luma = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
        *pixel = image::Rgba([luma, luma, luma, a]);
    }
    Some(crate::ui::render_image(rgba))
}

/// Centered watermark block: the mark on top, a caption, then a muted hint.
/// Used for the no-windows state and for the paused-member state while the menu
/// is open, so both read as intentional rather than a blank panel.
fn empty_state(cx: &App, icon: Option<Arc<RenderImage>>, caption: &str, hint: &str) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .when_some(icon, |empty, icon| {
            empty.child(Image::new("empty-mark", icon).w(px(128.0)).h(px(128.0)).opacity(0.18))
        })
        .child(Caption::new(caption.to_string()))
        .child(
            div()
                .text_size(cx.theme().text_size(TextSize::Xs))
                .text_color(cx.theme().colors.fg_muted)
                .child(hint.to_string()),
        )
}

impl Render for HostWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controller = self.controller.clone();

        if !self.ready {
            if let Some(hwnd) = hwnd_of(window) {
                let titlebar_h = {
                    let theme = cx.theme();
                    theme.titlebar_height().to_pixels(theme.base_rem()).as_f32()
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

        // Route the OS close (Alt+F4, taskbar) to a clean shutdown once.
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
        for TabInfo { id, title, icon } in &info.tabs {
            let icon = match (info.icons_in_tabs, icon) {
                (true, Some(image)) => TabIcon::Real(image.clone()),
                _ => TabIcon::Glyph(IconName::AppWindow),
            };
            tab_bar = tab_bar.tab(SlateTab::new(id.0.to_string(), title.clone(), icon));
        }
        if let Some(active) = &info.active {
            tab_bar = tab_bar.selected(SharedString::from(active.0.to_string()));
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
        // `+` arms pick mode: hover another window and press TAB to add it.
        tab_bar = tab_bar.on_add({
            let ctrl = controller.clone();
            move |_w, cx| {
                ctrl.update(cx, |c, cx| c.dispatch(Cmd::BeginPick, cx));
            }
        });

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
        let theme_button_left = {
            let ctrl = controller.clone();
            move |_w: &mut Window, cx: &mut App| {
                ctrl.update(cx, |c, cx| {
                    c.dispatch(Cmd::SetThemeButtonRight(false), cx);
                    c.dispatch(Cmd::SetOverlay(false), cx);
                });
            }
        };
        let theme_button_right = {
            let ctrl = controller.clone();
            move |_w: &mut Window, cx: &mut App| {
                ctrl.update(cx, |c, cx| {
                    c.dispatch(Cmd::SetThemeButtonRight(true), cx);
                    c.dispatch(Cmd::SetOverlay(false), cx);
                });
            }
        };
        // Quit through the controller so every hosted window is unregistered and
        // restored before the process exits (a bare `process::exit` would leave
        // them frame-stripped until the janitor replays).
        let quit = {
            let ctrl = controller.clone();
            move |window: &mut Window, cx: &mut App| {
                ctrl.update(cx, |c, cx| c.dispatch(Cmd::Shutdown, cx));
                window.remove_window();
            }
        };
        let app_menu = Menu::new()
            .item(
                MenuItem::new("Add window…")
                    .icon(IconName::Plus)
                    .keys("Tab")
                    .on_click(begin_pick),
            )
            .item(MenuItem::new("Refresh windows").on_click(refresh))
            .separator()
            .item(
                MenuItem::check("Show window icon in tabs", info.icons_in_tabs)
                    .on_click(toggle_icons),
            )
            .item(MenuItem::submenu(
                "Theme button",
                Menu::new()
                    .item(
                        MenuItem::radio("Left", !info.theme_button_right)
                            .on_click(theme_button_left),
                    )
                    .item(
                        MenuItem::radio("Right", info.theme_button_right)
                            .on_click(theme_button_right),
                    ),
            ))
            .separator()
            .item(MenuItem::new("Quit Slate").on_click(quit));
        let menu = OverflowMenu::new("host-menu", app_menu)
            // Cross when open, burgers when closed, mirroring the chevron swap.
            .icon(if info.overlay {
                IconName::X
            } else {
                IconName::Menu
            })
            .tooltip("Slate menu");

        // Hide the hosted member while the menu is open: an owned window always
        // paints above the host, so it would otherwise cover the dropdown.
        // Dispatch a live toggle (`ToggleOverlay`) rather than a captured
        // `!info.overlay`: rapid clicks outpace re-renders, and toggling from a
        // stale value desynced the flag from the menu, leaving the member hidden.
        let menu_wrap = {
            let ctrl = controller.clone();
            div()
                .id("menu-overlay-wrap")
                .flex_none()
                .occlude()
                .on_click(move |_ev, _w, cx| {
                    cx.stop_propagation();
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::ToggleOverlay, cx));
                })
                .child(menu)
        };

        // Custom dropdown, not ely's `OverflowMenu`: its rows can only carry a
        // glyph (never the host's real `RenderImage`), and its panel width can't
        // be capped. This lists every tab with the same icon as the strip, opens
        // under the chevron, and hides the member while open (an owned window
        // paints above the host, so it would cover the panel otherwise).
        let tab_anchor =
            window.use_keyed_state(("host-tab-overflow-anchor", 0usize), cx, |_, _| Bounds::default());
        let viewport = window.viewport_size();
        // Always rendered so the button stays put for UI consistency, even with
        // no tabs (the panel then shows a muted placeholder row).
        let tab_overflow = {
            let theme = cx.theme();
            let colors = &theme.colors;
            let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
            let row_h = theme.control_height(ControlSize::Md);
            let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
            let text_sm = theme.text_size(TextSize::Sm);
            let shadow = theme.elevation(Elevation::Floating);
            let open = info.tab_menu_open;

            // Open shows a horizontal bar (an "open" affordance); closed shows
            // the chevron. Toggle from the controller's *current* value so a
            // second click always closes rather than reopening a stale panel.
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

            // Rows: the host's real icon when available, else a generic glyph.
            let rows = info.tabs.iter().enumerate().map(|(ix, tab)| {
                let (id, title, icon) = (tab.id, &tab.title, &tab.icon);
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
                    .h(row_h)
                    .rounded(rad_sm)
                    .text_size(text_sm)
                    .text_color(if active { colors.fg } else { colors.fg_muted })
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.hover))
                    .on_click(move |_, _w, cx| {
                        ctrl.update(cx, |c, cx| {
                            if let Some(g) = group {
                                c.dispatch(Cmd::SelectTab { group: g, tab: tab_id }, cx);
                            }
                            c.dispatch(Cmd::SetTabMenu(false), cx);
                        });
                    })
                    .map(|el| match (info.icons_in_tabs, icon) {
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
                    .child(div().flex_1().min_w_0().child(Ellipsis::new(title.clone())))
            });

            let empty_row = info.tabs.is_empty().then(|| {
                div()
                    .px_2()
                    .py_1()
                    .text_size(text_sm)
                    .text_color(colors.fg_subtle)
                    .child("No windows open")
            });

            let panel = div()
                .flex()
                .flex_col()
                .gap_0p5()
                .p_1()
                .w(px(220.0))
                .rounded(rad_lg)
                .bg(colors.overlay)
                .border_1()
                .border_color(colors.border)
                .shadow(shadow)
                .children(rows)
                .children(empty_row);

            let mut wrap = div()
                .id("tab-overflow-wrap")
                .relative()
                .flex_none()
                .occlude()
                // Keep clicks on the chevron/panel from bubbling to the content
                // root's dismiss handler (which would fight the toggle).
                .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
                .child(chevron)
                // Measures the chevron so the panel opens under it.
                .child({
                    let anchor = tab_anchor.clone();
                    canvas(
                        move |bounds, window, cx| {
                            let changed =
                                anchor.update(cx, |a, _| std::mem::replace(a, bounds) != bounds);
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

            if open {
                let at = tab_anchor.read(cx).bottom_left();
                // Full-window transparent catcher: any click outside the panel
                // closes it (the host hides the member, but its own chrome still
                // sits under this layer).
                let scrim = raise(
                    ("host-tab-overflow-scrim", 0usize),
                    anchored().position(point(px(0.0), px(0.0))).child(
                        div()
                            .w(viewport.width)
                            .h(viewport.height)
                            .on_mouse_down(MouseButton::Left, {
                                let ctrl = controller.clone();
                                move |_, _w, cx| {
                                    // Stop here so the click never reaches the
                                    // chevron underneath; otherwise the scrim
                                    // closes (false) and the chevron's on_click
                                    // toggles it straight back open.
                                    cx.stop_propagation();
                                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::SetTabMenu(false), cx));
                                }
                            }),
                    ),
                );
                let placed = raise(
                    ("host-tab-overflow-panel", 0usize),
                    anchored()
                        .position(at)
                        .anchor(Anchor::TopLeft)
                        .snap_to_window()
                        .child(panel),
                )
                .with_priority(1);
                wrap = wrap.child(scrim).child(placed);
            }
            wrap
        };

        let mode = cx.theme().mode();
        // Built on demand so it can sit on either side of the strip (see
        // `theme_button_right`); exactly one call site runs each render.
        let make_theme = move || {
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
                .on_click(|_event, _window, cx| {
                    let next = match cx.theme().mode() {
                        Mode::Dark => Mode::Light,
                        Mode::Light => Mode::Dark,
                    };
                    Theme::set_mode_now(next, cx);
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

        // Interactive clusters are wrapped in `occlude()` so the drag region's
        // window-control hitbox is dropped while hovering them; otherwise
        // `WM_NCHITTEST` reports HTCAPTION across the bar and buttons never get
        // the click. The tab strip itself is not occluded (each tab is), so its
        // empty area still drags the window; the fixed gutter keeps a drag handle
        // even when tabs fill the strip.
        // The icon buttons are shorter than the bar, so they sit with a centered
        // gap on top/bottom. The old fixed `pl_2` (8px) was wider than that gap,
        // which made the bar look heavy on the left; `pr_1` (4px) kept the
        // caption buttons from reaching the right edge. Match the icon buttons'
        // own gap on the left and leave the controls flush right.
        let leading_inset = {
            let theme = cx.theme();
            let rem = theme.base_rem();
            let bar = theme.titlebar_height().to_pixels(rem).as_f32();
            let icon = theme.control_height(ControlSize::Md).to_pixels(rem).as_f32();
            ((bar - icon) / 2.0).max(0.0)
        };
        let mut title_bar = drag_region("host-titlebar", window, cx)
            .flex()
            .items_center()
            .gap_1()
            .flex_none()
            .h(titlebar_h)
            .bg(bg)
            .pl(px(leading_inset))
            .child(menu_wrap);
        if !info.theme_button_right {
            title_bar = title_bar.child(make_theme());
        }
        title_bar = title_bar
            // `flex_1` + `min_w_0` gives the tab bar a definite width to divide
            // among its tabs, so they shrink together as more open.
            .child(div().flex_1().min_w_0().child(tab_bar))
            .child(tab_overflow);
        if info.theme_button_right {
            title_bar = title_bar.child(make_theme());
        }
        title_bar = title_bar
            .child(div().flex_none().w(px(64.0)))
            .child(div().flex_none().occlude().child(controls));

        // Any click that reaches the root (i.e. not swallowed by the menu
        // wrapper) means the menu overlay is no longer wanted: re-show the
        // hosted member.
        let dismiss = {
            let ctrl = controller.clone();
            move |_ev: &gpui::MouseDownEvent, _w: &mut Window, cx: &mut App| {
                ctrl.update(cx, |c, cx| {
                    if c.host_info().overlay {
                        c.dispatch(Cmd::SetOverlay(false), cx);
                    }
                });
            }
        };

        // Escape closes either overlay. ely's menu closes itself on Escape but
        // doesn't clear our flag, which would leave the member hidden and the
        // burger stuck as a cross; clearing here keeps the two in sync.
        let escape = {
            let ctrl = controller.clone();
            move |ev: &gpui::KeyDownEvent, _w: &mut Window, cx: &mut App| {
                if ev.keystroke.key == "escape" {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::SetOverlay(false), cx);
                        c.dispatch(Cmd::SetTabMenu(false), cx);
                    });
                }
            }
        };

        let content = div()
            .id("host-content")
            .size_full()
            .flex()
            .flex_col()
            .bg(bg)
            .on_mouse_down(MouseButton::Left, dismiss)
            .on_key_down(escape)
            .child(title_bar);

        let body = match (info.tabs.is_empty(), info.overlay || info.tab_menu_open) {
            (true, _) => empty_state(
                cx,
                self.empty_icon.clone(),
                "No windows yet",
                "Click + or press Tab to add a window",
            ),
            // The menu open hides the member (it would paint over the dropdown),
            // leaving the content blank. Show the empty-state art there instead
            // with the open count, so the window doesn't read as broken.
            (false, true) => {
                let n = info.tabs.len();
                let (caption, hint) = if n == 1 {
                    (
                        "1 window open".to_string(),
                        "Hosted window paused while the menu is open".to_string(),
                    )
                } else {
                    (
                        format!("{n} windows open"),
                        "Hosted windows paused while the menu is open".to_string(),
                    )
                };
                empty_state(cx, self.empty_icon.clone(), &caption, &hint)
            }
            (false, false) => div().flex_1(),
        };

        // Toasts are shown here, in the host. When the host is empty there is no
        // hosted member, so the toast is fully visible; once a window is hosted
        // the member covers the content area and the toast is simply not seen.
        let toaster = self.controller.read(cx).toaster();
        content
            .child(body)
            .child(ToastViewport::new("host-toasts", &toaster))
            .into_any_element()
    }
}
