use std::sync::Arc;

use ely_gpui_component::primitives::Image;
use ely_gpui_component::theme::{ActiveTheme, Radius, TextSize};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AnyElement, App, Div, FontWeight, ParentElement, RenderImage, SharedString, Styled,
};

pub(super) fn grayscale_mark() -> Option<Arc<RenderImage>> {
    const PNG: &[u8] = include_bytes!(env!("SLATE_LOGO"));
    let mut rgba = image::load_from_memory(PNG).ok()?.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let luma = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
        *pixel = image::Rgba([luma, luma, luma, a]);
    }
    Some(crate::ui::render_image(rgba))
}

/// A single key cap, styled like the caps ely draws for shortcuts.
pub(super) fn key_cap(label: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .min_w(theme.text_size(TextSize::Xs) * 1.8)
        .px_1()
        .flex()
        .justify_center()
        .rounded(theme.radius(Radius::Sm))
        .border_1()
        .border_b_2()
        .border_color(theme.colors.border_strong)
        .bg(theme.colors.surface)
        .text_size(theme.text_size(TextSize::Xs))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.colors.fg_muted)
        .child(label.into())
}

pub(super) fn empty_state(
    cx: &App,
    icon: Option<Arc<RenderImage>>,
    caption: AnyElement,
    hint: AnyElement,
) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .when_some(icon, |empty, icon| {
            empty.child(
                Image::new("empty-mark", icon)
                    .w(px(128.0))
                    .h(px(128.0))
                    .opacity(0.18),
            )
        })
        .child(caption)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(cx.theme().text_size(TextSize::Xs))
                .text_color(cx.theme().colors.fg_muted)
                .child(hint),
        )
}
