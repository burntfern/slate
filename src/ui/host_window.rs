//! US-like host window. The title bar is `[☰ menu][tabs][controls]`; the
//! TabBar's own `+` arms pick mode (hover an app, press TAB to host it). The
//! content rect below is where the active hosted member is pinned.

use std::rc::Rc;
use std::sync::Arc;

use ely_gpui_component::{
    buttons::IconButton,
    feedback::ToastViewport,
    forms::TextInput,
    menus::{Menu, MenuItem, OverflowMenu},
    motion,
    overlays::PromptDialog,
    primitives::{Icon, IconName, Image},
    shell::{drag_region, WindowControls},
    theme::{ActiveTheme, ControlSize, Elevation, IconSize, Mode, Radius, TextSize, Theme},
    typography::{Caption, Ellipsis},
};
use gpui::{
    canvas, div, point, prelude::*, px, Animation, AnimationExt, App, AppContext, Bounds,
    ClipboardItem, Context, Div, Entity, FontWeight, IntoElement, MouseButton, ParentElement,
    Pixels, Point, Render, RenderImage, SharedString, Size, Styled, Subscription, Window,
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

/// A callback an overlay runs on dismissal or on choosing a menu item.
type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// A dropdown panel, placed as a plain absolute child of the content root — not
/// `raise`/`anchored` nested inside the drag region, whose content mask clips it
/// and leaves its rows unclickable. Dismissal is handled by the content root's
/// own `on_mouse_down` (which closes every open dropdown on an outside press);
/// the panel stops propagation so a press on a row doesn't reach that handler
/// and unmount the menu before the row's `on_click` runs. An owned member would
/// still paint over the panel, so the caller hides the member while it's shown.
fn overlay_layer(
    tag: &'static str,
    at: Point<Pixels>,
    width: f32,
    viewport: Size<Pixels>,
    enter: Animation,
    panel: Div,
) -> gpui::AnyElement {
    const MARGIN: f32 = 8.0;
    // Keep the panel inside the window: right-align it when the anchor is close
    // to the right edge (the `+` and chevron both live there).
    let left = at
        .x
        .as_f32()
        .min(viewport.width.as_f32() - width - MARGIN)
        .max(0.0);
    div()
        .absolute()
        .left(px(left))
        .top(at.y)
        .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
        .with_animation((tag, 0usize), enter, |panel, t| {
            panel.opacity(t).mt(motion::NUDGE * (t - 1.0))
        })
        .child(panel)
        .into_any_element()
}

/// The dropdown entrance animation: ely's `FAST` duration with an ease-out-cubic
/// fade. Built before the panel makes theme borrows, so it only needs `&App`.
fn enter_animation(cx: &App) -> Animation {
    Animation::new(motion::duration(motion::FAST, cx)).with_easing(motion::ease_out_cubic)
}

pub struct HostWindow {
    controller: Entity<Controller>,
    ready: bool,
    close_routed: bool,
    /// A muted, grayscale copy of the Slate mark shown as the empty-state art.
    empty_icon: Option<Arc<RenderImage>>,
    /// The rename dialog's input box, created on first use (it needs a window).
    rename_field: Option<Entity<TextInput>>,
    /// The tab whose title was last loaded into `rename_field`, so we only
    /// reset the text when the dialog opens for a new target.
    rename_primed: Option<TabId>,
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
            rename_field: None,
            rename_primed: None,
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
        for TabInfo { id, title, icon, .. } in &info.tabs {
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
        // A right-click on a tab opens the context menu exactly at the pointer;
        // `+` (rendered in the title bar, not here) arms the window picker.
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
                MenuItem::new("Pick a window…")
                    .icon(IconName::Plus)
                    .keys("Tab")
                    .on_click(begin_pick),
            )
            .item(MenuItem::new("Refresh window list").on_click(refresh))
            .separator()
            .item(
                MenuItem::check("Show window icons in tabs", info.icons_in_tabs)
                    .on_click(toggle_icons),
            )
            .item(
                MenuItem::check("Show tab count badge", info.show_tab_badge)
                    .on_click(toggle_badge),
            )
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
        // The chevron panel is built here but attached to the content root later
        // (as a plain absolute child) so the title bar's drag mask can't clip it.
        let mut chevron_overlay: Option<gpui::AnyElement> = None;
        // Always rendered so the button stays put for UI consistency, even with
        // no tabs (the panel then shows a muted placeholder row).
        let tab_overflow = {
            let theme = cx.theme();
            let colors = &theme.colors;
            let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
            let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
            let (text_sm, text_xs) =
                (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
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
                // Keep clicks on the chevron/panel from bubbling to the content
                // root's dismiss handler (which would fight the toggle).
                .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
                .child({
                    // A small count pill rides the chevron's corner so the
                    // collapsed overflow still reports how many tabs there are.
                    // Hand-rolled (not `CountBadge`) so it stays compact and can
                    // be turned off in the menu.
                    let n = info.tabs.len();
                    if info.show_tab_badge && n > 0 {
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
                            // Explicit tiny size; `TextSize::Xs` (11px) fills the
                            // whole pill and reads bulky at a glance.
                            .text_size(px(8.5))
                            .line_height(px(8.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label);
                        div()
                            .relative()
                            .flex_none()
                            .child(chevron)
                            // Tucked into the corner so it never protrudes.
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-3.0))
                                    .right(px(-3.0))
                                    .child(pill),
                            )
                            .into_any_element()
                    } else {
                        chevron.into_any_element()
                    }
                })
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
                chevron_overlay = Some(overlay_layer(
                    "host-tab-overflow",
                    at,
                    300.0,
                    viewport,
                    enter_animation(cx),
                    panel,
                ));
            }
            wrap
        };

        // The `+` window picker. Hand-rolled for the same two reasons as the tab
        // overflow above: rows carry each target's real icon, and the member is
        // hidden while it's open (an owned window would paint over the panel).
        let picker_anchor =
            window.use_keyed_state(("host-picker-anchor", 0usize), cx, |_, _| Bounds::default());
        let mut picker_overlay: Option<gpui::AnyElement> = None;
        let picker_wrap = div()
            .id("picker-wrap")
            .relative()
            .flex_none()
            .occlude()
            // Keep the press off the content root's dismiss handler.
            .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
            .child(
                IconButton::new("host-add-window", IconName::Plus)
                    .tooltip("Add a window")
                    // Toggle from the controller's current value so a second
                    // click always closes rather than reopening a stale panel.
                    .on_click({
                        let ctrl = controller.clone();
                        move |_, _w, cx| {
                            ctrl.update(cx, |c, cx| c.dispatch(Cmd::TogglePicker, cx));
                        }
                    }),
            )
            .child({
                let anchor = picker_anchor.clone();
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
        if info.picker_open {
            let theme = cx.theme();
            let colors = &theme.colors;
            let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
            let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
            let (text_sm, text_xs) =
                (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
            let shadow = theme.elevation(Elevation::Floating);

            let mut rows: Vec<gpui::AnyElement> = Vec::with_capacity(info.picker_rows.len() + 1);
            // Arms pick mode and gets out of the way so the user can hover a
            // target window and press Tab.
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
                    .child(Icon::new(IconName::Plus).size(IconSize::Sm).color(colors.fg_subtle))
                    .child(div().flex_1().min_w_0().child("Pick a window with"))
                    .child(div().text_size(text_xs).text_color(colors.fg_subtle).child("Tab"))
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
                    // A window that can't be hosted reads as disabled.
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
                        Icon::new(IconName::AppWindow).size(IconSize::Sm).color(colors.fg_subtle),
                    ),
                };
                // Blocked by an eligibility rule: show why instead of the exe,
                // so the reason is legible rather than squeezed into a tag.
                let detail = row
                    .reason
                    .clone()
                    .unwrap_or_else(|| row.exe.clone());
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

            let at = picker_anchor.read(cx).bottom_left();
            picker_overlay = Some(overlay_layer(
                "host-picker",
                at,
                300.0,
                viewport,
                enter_animation(cx),
                panel,
            ));
        }

        // The tab right-click menu. Hand-rolled for the same occlusion reason:
        // ely's `ContextMenu` panel would sit under a hosted member.
        let tab_menu_panel = info.tab_menu.as_ref().and_then(|menu| {
            let group = group?;
            let tab = menu.tab;
            let theme = cx.theme();
            let colors = &theme.colors;
            let (rad_sm, rad_lg) = (theme.radius(Radius::Sm), theme.radius(Radius::Lg));
            let (text_sm, text_xs) =
                (theme.text_size(TextSize::Sm), theme.text_size(TextSize::Xs));
            let row_h = theme.control_height(ControlSize::Md);
            let shadow = theme.elevation(Elevation::Floating);
            let ix = info.tabs.iter().position(|t| t.id == tab);
            let can_right = ix.is_some_and(|ix| ix + 1 < info.tabs.len());
            let can_left = ix.is_some_and(|ix| ix > 0);

            let item = |id: &'static str,
                        label: &'static str,
                        desc: Option<&'static str>,
                        icon: IconName,
                        danger: bool,
                        enabled: bool,
                        run: Action| {
                let text_color = if danger { colors.danger } else { colors.fg_muted };
                let icon_color = if danger { colors.danger } else { colors.fg_subtle };
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .px_2()
                    .min_h(row_h)
                    // Only described rows need breathing room; a plain row stays
                    // exactly `row_h` so the menu doesn't grow a stray pixel.
                    .when(desc.is_some(), |el| el.py_1())
                    .rounded(rad_sm)
                    .text_size(text_sm)
                    .text_color(text_color)
                    // A disabled entry reads as disabled: dimmed, no pointer,
                    // no hover, and its click is a no-op.
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
            };

            // Ungroup: return the window to a standalone top-level window.
            let ungroup = {
                let ctrl = controller.clone();
                Rc::new(move |_w: &mut Window, cx: &mut App| {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::Ungroup { group, tab }, cx);
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    });
                }) as Action
            };
            // Close: really close the window (post `WM_CLOSE`).
            let close = {
                let ctrl = controller.clone();
                Rc::new(move |_w: &mut Window, cx: &mut App| {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::CloseTab { group, tab }, cx);
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    });
                }) as Action
            };
            let others = {
                let ctrl = controller.clone();
                Rc::new(move |_w: &mut Window, cx: &mut App| {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::CloseOtherTabs { group, tab }, cx);
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    });
                }) as Action
            };
            let left = {
                let ctrl = controller.clone();
                Rc::new(move |_w: &mut Window, cx: &mut App| {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::CloseTabsToLeft { group, tab }, cx);
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    });
                }) as Action
            };
            let right = {
                let ctrl = controller.clone();
                Rc::new(move |_w: &mut Window, cx: &mut App| {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::CloseTabsToRight { group, tab }, cx);
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    });
                }) as Action
            };
            let rename = {
                let ctrl = controller.clone();
                Rc::new(move |_w: &mut Window, cx: &mut App| {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::BeginRename { group, tab }, cx);
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    });
                }) as Action
            };
            // Copy exe path: pull the target tab's executable path from the
            // snapshot, then drop it on the clipboard.
            let exe_path = ix
                .and_then(|ix| info.tabs.get(ix))
                .map(|t| t.exe_path.clone())
                .unwrap_or_default();
            let copy_path = {
                let path = exe_path.clone();
                let ctrl = controller.clone();
                Rc::new(move |_w: &mut Window, cx: &mut App| {
                    cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                    ctrl.update(cx, |c, cx| c.dispatch(Cmd::CloseTabMenu, cx));
                }) as Action
            };

            // Non-destructive actions first, then a divider, then the
            // destructive Close pinned to the bottom.
            let divider = |id: &'static str| {
                div().id(id).my_1().h(px(1.0)).w_full().bg(colors.border)
            };
            let rows = vec![
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
                    info.tabs.len() > 1,
                    others,
                ),
                item(
                    "ctx-left",
                    "Close to the left",
                    None,
                    IconName::ChevronLeft,
                    false,
                    can_left,
                    left,
                ),
                item(
                    "ctx-right",
                    "Close to the right",
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

            Some((menu.at, panel))
        });

        let mode = cx.theme().mode();
        // The theme toggle always sits at the right of the title bar, just
        // before the drag gutter and the window controls.
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
        // Left of the bar is the app menu only; the tabs take the middle; the
        // right cluster is `+`, the overflow chevron, the theme toggle, a drag
        // gutter, then the window controls. While pick mode is armed the
        // spinner takes the `+` button's exact slot.
        let (spinner_color, pick_side) = {
            let theme = cx.theme();
            (
                theme.colors.fg_muted,
                theme.control_height(ControlSize::default()),
            )
        };
        let pick_slot: gpui::AnyElement = if info.picking {
            // Occupy the `+` button's exact box so the row's gaps don't shift.
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
            picker_wrap.into_any_element()
        };
        let title_bar = drag_region("host-titlebar", window, cx)
            .flex()
            .items_center()
            .gap_1()
            .flex_none()
            .h(titlebar_h)
            .bg(bg)
            .pl(px(leading_inset))
            .child(menu_wrap)
            // `flex_1` + `min_w_0` gives the tab bar a definite width to divide
            // among its tabs, so they shrink together as more open.
            .child(div().flex_1().min_w_0().child(tab_bar))
            .child(pick_slot)
            .child(tab_overflow)
            .child(make_theme())
            .child(div().flex_none().w(px(64.0)))
            .child(div().flex_none().occlude().child(controls));

        // Any press that reaches the root (i.e. not swallowed by a dropdown
        // panel or one of the triggers, which stop propagation) means the open
        // overlay is no longer wanted: close it and re-show the hosted member.
        let dismiss = {
            let ctrl = controller.clone();
            let (overlay, tab_menu, picker, ctx) = (
                info.overlay,
                info.tab_menu_open,
                info.picker_open,
                info.tab_menu.is_some(),
            );
            move |_ev: &gpui::MouseDownEvent, _w: &mut Window, cx: &mut App| {
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
                    if ctx {
                        c.dispatch(Cmd::CloseTabMenu, cx);
                    }
                });
            }
        };

        // Escape closes every transient overlay and cancels an in-flight rename.
        // ely's menu closes itself on Escape but doesn't clear our flag, which
        // would leave the member hidden and the burger stuck as a cross;
        // clearing here keeps the two in sync.
        let escape = {
            let ctrl = controller.clone();
            move |ev: &gpui::KeyDownEvent, _w: &mut Window, cx: &mut App| {
                if ev.keystroke.key == "escape" {
                    ctrl.update(cx, |c, cx| {
                        c.dispatch(Cmd::SetOverlay(false), cx);
                        c.dispatch(Cmd::SetTabMenu(false), cx);
                        c.dispatch(Cmd::SetPickerOpen(false), cx);
                        c.dispatch(Cmd::CancelRename, cx);
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

        // Every state that hides the hosted member: the empty-state art stands
        // in so the window doesn't read as broken.
        let overlaying = info.overlay
            || info.tab_menu_open
            || info.picker_open
            || info.tab_menu.is_some()
            || info.renaming.is_some();
        let body = match (info.tabs.is_empty(), overlaying) {
            (true, _) => empty_state(
                cx,
                self.empty_icon.clone(),
                "No windows yet",
                "Click + to add a window, or press Tab to pick one",
            ),
            // The menu open hides the member (it would paint over the dropdown),
            // leaving the content blank. Show the empty-state art there instead
            // with the open count, so the window doesn't read as broken.
            (false, true) => {
                let n = info.tabs.len();
                let (caption, hint) = if n == 1 {
                    (
                        "1 window open".to_string(),
                        "Your window is hidden while a menu is open".to_string(),
                    )
                } else {
                    (
                        format!("{n} windows open"),
                        "Your windows are hidden while a menu is open".to_string(),
                    )
                };
                empty_state(cx, self.empty_icon.clone(), &caption, &hint)
            }
            (false, false) => div().flex_1(),
        };

        // The tab right-click menu floats at the pointer and dismisses on any
        // outside click, exactly like the picker.
        let tab_menu_overlay = tab_menu_panel.map(|(at, panel)| {
            overlay_layer(
                "host-tab-menu",
                point(px(at.0), px(at.1)),
                220.0,
                viewport,
                enter_animation(cx),
                panel,
            )
        });

        // Rename uses ely's `PromptDialog`, which needs a caller-owned
        // `TextInput` entity. It is created on first use (it needs a window).
        // The dialog overlays the content; the member is hidden while it's open.
        let rename_dialog = if let (Some(tab_id), Some(group)) = (info.renaming, group) {
            let field = self
                .rename_field
                .get_or_insert_with(|| cx.new(|cx| TextInput::new(window, cx)))
                .clone();
            // Prime the box once per target: fill it with the current title and
            // select it all, so typing replaces rather than appends.
            if self.rename_primed != Some(tab_id) {
                self.rename_primed = Some(tab_id);
                let current = info
                    .tabs
                    .iter()
                    .find(|t| t.id == tab_id)
                    .map(|t| t.title.clone())
                    .unwrap_or_default();
                let end = current.len();
                field.update(cx, |f, cx| {
                    f.set_text(current, cx);
                    f.select(0..end, cx);
                });
            }
            Some(
                PromptDialog::new(
                    ("host-rename", 0usize),
                    "Rename tab",
                    &field,
                    {
                        let ctrl = controller.clone();
                        move |_w, cx| {
                            ctrl.update(cx, |c, cx| c.dispatch(Cmd::CancelRename, cx));
                        }
                    },
                )
                .label("Tab name")
                .submit("Rename")
                .check(|text: &str| {
                    if text.trim().is_empty() {
                        Err(SharedString::from("Name cannot be empty"))
                    } else {
                        Ok(())
                    }
                })
                .on_submit({
                    let ctrl = controller.clone();
                    move |text, _w, cx| {
                        let title = text.to_string();
                        ctrl.update(cx, |c, cx| {
                            c.dispatch(Cmd::RenameTab { group, tab: tab_id, title }, cx);
                            c.dispatch(Cmd::CancelRename, cx);
                        });
                    }
                })
                .into_any_element(),
            )
        } else {
            // Dialog closed: forget the priming so reopening reloads the title.
            self.rename_primed = None;
            None
        };

        // Toasts are shown here, in the host. When the host is empty there is no
        // hosted member, so the toast is fully visible; once a window is hosted
        // the member covers the content area and the toast is simply not seen.
        let toaster = self.controller.read(cx).toaster();
        let mut content = content.child(body);
        // Dropdowns ride on the content root (not the title bar) so the drag
        // region's mask can't clip them; the member is hidden while any is open.
        if let Some(overlay) = picker_overlay {
            content = content.child(overlay);
        }
        if let Some(overlay) = chevron_overlay {
            content = content.child(overlay);
        }
        if let Some(overlay) = tab_menu_overlay {
            content = content.child(overlay);
        }
        if let Some(dialog) = rename_dialog {
            content = content.child(dialog);
        }
        content
            .child(ToastViewport::new("host-toasts", &toaster))
            .into_any_element()
    }
}
