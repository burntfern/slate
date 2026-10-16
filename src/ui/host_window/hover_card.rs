use ely_gpui_component::primitives::Image;
use ely_gpui_component::theme::{ActiveTheme, Radius, TextSize};
use ely_gpui_component::typography::Ellipsis;
use gpui::prelude::*;
use gpui::{div, px, AnimationExt, App, Pixels, Size, Window};

use super::overlays::enter_animation;
use crate::controller::{HoverPreview, TabInfo};

pub(super) fn hover_card(
    cx: &App,
    preview: &HoverPreview,
    tab: Option<&TabInfo>,
    viewport: Size<Pixels>,
    on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    const CARD_W: f32 = 224.0;
    const IMAGE_H: f32 = 126.0;
    const MARGIN: f32 = 8.0;
    const GAP: f32 = 6.0;
    let theme = cx.theme();
    let colors = &theme.colors;
    let rad_lg = theme.radius(Radius::Lg);
    let (x, y, _w, h) = preview.anchor;
    let left = x.min(viewport.width.as_f32() - CARD_W - MARGIN).max(MARGIN);

    let image = match preview.image.clone() {
        Some(img) => Image::new(("host-hover-shot", 0usize), img)
            .w_full()
            .h(px(IMAGE_H))
            .into_any_element(),
        None => div()
            .w_full()
            .h(px(IMAGE_H))
            .flex()
            .items_center()
            .justify_center()
            .bg(colors.sunken)
            .when_some(tab.and_then(|t| t.icon.clone()), |area, icon| {
                area.child(
                    Image::new(("host-hover-icon", 0usize), icon)
                        .w(px(48.0))
                        .h(px(48.0)),
                )
            })
            .into_any_element(),
    };

    let title = tab
        .map(|t| t.title.clone())
        .unwrap_or_else(|| "Window".to_string());
    let footer = div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .border_t_1()
        .border_color(colors.border)
        .when_some(tab.and_then(|t| t.icon.clone()), |row, icon| {
            row.child(
                Image::new(("host-hover-tab-icon", 0usize), icon)
                    .w(px(16.0))
                    .h(px(16.0)),
            )
        })
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_size(theme.text_size(TextSize::Sm))
                .text_color(colors.fg)
                .child(Ellipsis::new(title)),
        );

    let card = div()
        .w(px(CARD_W))
        .rounded(rad_lg)
        .bg(colors.overlay)
        .border_1()
        .border_color(colors.border)
        .overflow_hidden()
        .child(image)
        .child(footer);

    div()
        .id(("host-hover", preview.tab.0 as usize))
        .absolute()
        .left(px(left))
        .top(px(y + h))
        .pt(px(GAP))
        .on_hover(on_hover)
        .with_animation(
            ("host-hover-anim", preview.tab.0 as usize),
            enter_animation(cx),
            |el, t| el.opacity(t),
        )
        .child(card)
        .into_any_element()
}
