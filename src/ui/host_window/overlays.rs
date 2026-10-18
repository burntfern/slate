use std::rc::Rc;

use ely_gpui_component::motion;
use ely_gpui_component::theme::{ActiveTheme, Elevation, Radius};
use gpui::{
    div, px, Animation, AnimationExt, App, Div, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, Role, Size, StatefulInteractiveElement, Styled, Window,
};

pub(super) type Action = Rc<dyn Fn(&mut Window, &mut App)>;

pub(super) const MENU_W: f32 = 300.0;
pub(super) const MENU_W_SAVED: f32 = 280.0;
pub(super) const MENU_W_TAB_MENU: f32 = 260.0;

const ROW_H: f32 = 30.0;
const PANEL_PAD: f32 = 16.0;

pub(super) fn menu_panel(width: f32, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .p_1()
        .w(px(width))
        .rounded(theme.radius(Radius::Lg))
        .bg(theme.colors.overlay)
        .border_1()
        .border_color(theme.colors.border)
        .shadow(theme.elevation(Elevation::Floating))
}

pub(super) fn menu_height(rows: usize) -> f32 {
    rows as f32 * ROW_H + PANEL_PAD
}

pub(super) fn overlay_layer(
    tag: &'static str,
    at: Point<Pixels>,
    width: f32,
    height: f32,
    viewport: Size<Pixels>,
    enter: Animation,
    panel: Div,
) -> gpui::AnyElement {
    const MARGIN: f32 = 8.0;
    let vw = viewport.width.as_f32();
    let vh = viewport.height.as_f32();
    let avail = (vh - MARGIN * 2.0).max(0.0);
    let panel_h = height.min(avail);
    let left = at.x.as_f32().min(vw - width - MARGIN).max(0.0);
    let top = at.y.as_f32().min((vh - panel_h - MARGIN).max(0.0)).max(0.0);
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .on_mouse_down(MouseButton::Left, |_ev, _w, cx| cx.stop_propagation())
        .child(
            div()
                .id(tag)
                .role(Role::Menu)
                .max_h(px(avail))
                .overflow_y_scroll()
                .with_animation((tag, 0usize), enter, |panel, t| {
                    panel.opacity(t).mt(motion::NUDGE * (t - 1.0))
                })
                .child(panel),
        )
        .into_any_element()
}

pub(super) fn enter_animation(cx: &App) -> Animation {
    Animation::new(motion::duration(motion::FAST, cx)).with_easing(motion::ease_out_cubic)
}
