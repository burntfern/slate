//! Host tab bar. A copy of ely's `TabBar` with two changes Slate needs: tabs
//! can show a host's real window icon (an `Arc<RenderImage>`), and each tab
//! carries a right-click context hook (the title bar owns the `+` picker).

use std::rc::Rc;
use std::sync::Arc;

use ely_gpui_component::{
    buttons::close_button,
    motion,
    primitives::{DragGhost, Icon, IconName, Image},
    theme::{ActiveTheme, ControlSize, IconSize, Radius, TextSize},
    typography::Ellipsis,
};
use gpui::{
    canvas, div, prelude::*, Animation, AnimationExt, App, Bounds, ElementId, EntityId,
    FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point,
    RenderImage, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
};
/// What to draw at the start of a tab.
#[derive(Clone)]
pub enum TabIcon {
    /// A host's real icon.
    Real(Arc<RenderImage>),
    /// A generic glyph (icons disabled, or none available).
    Glyph(IconName),
}

/// One host tab.
#[derive(Clone)]
pub struct SlateTab {
    id: SharedString,
    title: SharedString,
    icon: TabIcon,
    /// Accent color drawn as a bar along the tab's top edge. `None` draws none.
    accent: Option<Hsla>,
}

impl SlateTab {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>, icon: TabIcon) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon,
            accent: None,
        }
    }

    /// The colored top edge for this tab.
    pub fn accent(mut self, color: Option<Hsla>) -> Self {
        self.accent = color;
        self
    }
}

struct TabDrag {
    owner: EntityId,
    from: usize,
}

type OnTab = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
type OnReorder = Rc<dyn Fn(usize, usize, &mut Window, &mut App)>;
type OnContext = Rc<dyn Fn(&SharedString, Point<Pixels>, &mut Window, &mut App)>;
/// Reports a tab hover changing, with the tab's bounds in window space (so a
/// caller can anchor something below it, e.g. a hover preview).
type OnTabHover = Rc<dyn Fn(&SharedString, bool, Bounds<Pixels>, &mut Window, &mut App)>;

/// Browser-style tabs across the top of the host window.
#[derive(IntoElement)]
pub struct SlateTabBar {
    id: ElementId,
    tabs: Vec<SlateTab>,
    selected: Option<SharedString>,
    on_select: Option<OnTab>,
    on_close: Option<OnTab>,
    on_reorder: Option<OnReorder>,
    on_context: Option<OnContext>,
    on_tab_hover: Option<OnTabHover>,
}

impl SlateTabBar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            tabs: Vec::new(),
            selected: None,
            on_select: None,
            on_close: None,
            on_reorder: None,
            on_context: None,
            on_tab_hover: None,
        }
    }

    pub fn tab(mut self, tab: SlateTab) -> Self {
        self.tabs.push(tab);
        self
    }

    pub fn selected(mut self, id: impl Into<SharedString>) -> Self {
        self.selected = Some(id.into());
        self
    }

    pub fn on_select(
        mut self,
        handler: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }

    pub fn on_close(
        mut self,
        handler: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    /// Lets tabs drag; reports `(from, to)` indexes on drop.
    pub fn on_reorder(
        mut self,
        handler: impl Fn(usize, usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_reorder = Some(Rc::new(handler));
        self
    }

    /// Reports a right-click on a tab, with the window-space position of the
    /// press so a context menu can open exactly there.
    pub fn on_context(
        mut self,
        handler: impl Fn(&SharedString, Point<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_context = Some(Rc::new(handler));
        self
    }

    /// Reports a tab's hover changing, with its window-space bounds.
    pub fn on_tab_hover(
        mut self,
        handler: impl Fn(&SharedString, bool, Bounds<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_tab_hover = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SlateTabBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let owner = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ())
            .entity_id();
        let theme = cx.theme();
        // Tabs share the bar's width: they grow to `wide` when there is room and
        // shrink together down to `narrow` as more open, exactly like ely's
        // default `TabBar`. No fixed width, so the strip never overflows.
        let (narrow, wide) = theme.tab_width();
        let icon_px = theme.icon_size(IconSize::Sm).to_pixels(theme.base_rem());
        let enter = motion::duration(motion::BASE, cx);
        let nudge = motion::NUDGE;
        let tabs = self.tabs.into_iter().enumerate().map(|(ix, tab)| {
            let hover_cb = self.on_tab_hover.clone();
            // Measures this tab's bounds each frame so a hover report can anchor
            // below it (a plain `on_hover` carries no geometry). Created before
            // `cx.theme()` so the borrow of `cx` doesn't overlap.
            let hover_bounds =
                window.use_keyed_state(("slate-tab-hover", ix), cx, |_, _| Bounds::default());
            let theme = cx.theme();
            let colors = &theme.colors;
            let selected = self.selected.as_ref() == Some(&tab.id);
            let accent = tab.accent;
            // An accented tab is washed with a light tint of its hue, not just
            // edged — the active tab reads a touch stronger than the rest.
            let tint = accent.map(|color| color.opacity(if selected { 0.24 } else { 0.15 }));
            let group = SharedString::from(format!("window-tab-{}", tab.id));
            let (select, close, reorder, context) = (
                self.on_select.clone(),
                self.on_close.clone(),
                self.on_reorder.clone(),
                self.on_context.clone(),
            );
            let (id, close_id) = (tab.id.clone(), tab.id.clone());
            let title = tab.title.clone();
            let ghost_icon = Some(IconName::AppWindow);
            let icon = tab.icon.clone();
            let (lit, edge) = (colors.hover, colors.focus);
            let glyph_color = if selected {
                colors.fg
            } else {
                colors.fg_subtle
            };
            div()
                .id(tab.id.clone())
                .group(group.clone())
                .flex()
                // `flex_1` + `min_w`/`max_w`: every tab takes an equal share of
                // the bar, growing up to `wide` and shrinking together to
                // `narrow` as more tabs open (the label ellipsizes), instead of
                // a fixed width that overflowed the title bar.
                .flex_1()
                .items_center()
                .gap_2()
                .min_w(narrow)
                .max_w(wide)
                .h(theme.control_height(ControlSize::Md))
                .pl_3()
                .pr_1()
                .rounded(theme.radius(Radius::Md))
                .border_1()
                .text_size(theme.text_size(TextSize::Sm))
                .map(|el| {
                    if selected {
                        el.bg(tint.unwrap_or(colors.surface))
                            .border_color(colors.border)
                            .text_color(colors.fg)
                            .font_weight(FontWeight::MEDIUM)
                    } else {
                        el.bg(tint.unwrap_or(gpui::transparent_black()))
                            .border_color(gpui::transparent_black())
                            .text_color(colors.fg_muted)
                            .hover(move |style| {
                                style.bg(tint.map(|c| c.opacity(0.28)).unwrap_or(lit))
                            })
                    }
                })
                .cursor_pointer()
                .occlude()
                .on_hover({
                    let state = hover_bounds.clone();
                    let hover_cb = hover_cb.clone();
                    let id = tab.id.clone();
                    move |hovered, window, cx| {
                        if let Some(hover_cb) = hover_cb.as_ref() {
                            let bounds = *state.read(cx);
                            hover_cb(&id, *hovered, bounds, window, cx);
                        }
                    }
                })
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_mouse_down(MouseButton::Middle, {
                    let close = close.clone();
                    let id = tab.id.clone();
                    move |_, window, cx| {
                        if let Some(close) = close.as_ref() {
                            close(&id, window, cx);
                        }
                    }
                })
                .on_mouse_down(MouseButton::Right, {
                    let context = context.clone();
                    let id = tab.id.clone();
                    move |event, window, cx| {
                        if let Some(context) = context.as_ref() {
                            context(&id, event.position, window, cx);
                        }
                    }
                })
                .on_click(move |_, window, cx| {
                    if let Some(select) = &select {
                        select(&id, window, cx);
                    }
                })
                .when_some(reorder, |el, reorder| {
                    el.on_drag(TabDrag { owner, from: ix }, move |_, _, _, cx| {
                        DragGhost::new(title.clone(), ghost_icon, cx)
                    })
                    .drag_over::<TabDrag>(move |style, drag, _, _| {
                        if drag.owner == owner {
                            style.border_color(edge)
                        } else {
                            style
                        }
                    })
                    .on_drop(move |drag: &TabDrag, window, cx| {
                        if drag.owner == owner && drag.from != ix {
                            reorder(drag.from, ix, window, cx);
                        }
                    })
                })
                .map(|el| match icon {
                    TabIcon::Real(image) => el.child(
                        Image::new(("tab-icon", ix), image)
                            .flex_none()
                            .w(icon_px)
                            .h(icon_px),
                    ),
                    TabIcon::Glyph(name) => {
                        el.child(Icon::new(name).size(IconSize::Sm).color(glyph_color))
                    }
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Ellipsis::new(tab.title.clone())),
                )
                .child({
                    // Tracks the tab's bounds (window space) for the hover
                    // report; absolute, so it doesn't affect the layout.
                    let measure = hover_bounds.clone();
                    canvas(
                        move |bounds, window, cx| {
                            let changed =
                                measure.update(cx, |b, _| std::mem::replace(b, bounds) != bounds);
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
                .when_some(close, |el, close| {
                    el.child(
                        div()
                            .when(!selected, |slot| {
                                slot.invisible().group_hover(group, |style| style.visible())
                            })
                            .child(
                                close_button(("close", ix))
                                    .size(ControlSize::Sm)
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        close(&close_id, window, cx);
                                    }),
                            ),
                    )
                })
                .with_animation(
                    SharedString::from(format!("tab-in-{}", tab.id)),
                    Animation::new(enter).with_easing(motion::ease_out_cubic),
                    move |el, t| el.opacity(t).mt(nudge * (1.0 - t)),
                )
        });
        // Tabs are direct children of the bar (as in ely's `TabBar`), so their
        // `flex_1` shares the bar's width. With few tabs they sit at their
        // capped `max_w` and `+` drifts right after the last one; as more open
        // they shrink to `narrow`. `w_full` makes the bar fill the title bar's
        // `flex_1` slot so the tabs have a definite width to divide.
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap_1()
            .w_full()
            .min_w_0()
            .children(tabs)
    }
}
