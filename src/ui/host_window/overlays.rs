use std::rc::Rc;

use ely_gpui_component::motion;
use gpui::{
    div, px, Animation, AnimationExt, App, Div, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, Size, Styled, Window,
};

pub(super) type Action = Rc<dyn Fn(&mut Window, &mut App)>;

pub(super) fn overlay_layer(
    tag: &'static str,
    at: Point<Pixels>,
    width: f32,
    viewport: Size<Pixels>,
    enter: Animation,
    panel: Div,
) -> gpui::AnyElement {
    const MARGIN: f32 = 8.0;
    let left =
        at.x.as_f32()
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

pub(super) fn enter_animation(cx: &App) -> Animation {
    Animation::new(motion::duration(motion::FAST, cx)).with_easing(motion::ease_out_cubic)
}
