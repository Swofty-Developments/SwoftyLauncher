use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::Duration;
use swofty_core::online::server::support::tier_name;

use crate::ui::scale::px;
use crate::ui::theme::Palette;

/// The widest the page's content grows, so the cards keep their shape on big screens.
pub(super) const MAX_WIDTH: f32 = 1180.;
/// Below this width the tier cards stack and the previews put their picture on top.
pub(super) const SIDE_BY_SIDE: f32 = 720.;
pub(super) const YELLOW: u32 = 0xffc933;
pub(super) const PINK: u32 = 0xff5fa2;
pub(super) const HOT_PINK: u32 = 0xff2d95;
pub(super) const PURPLE: u32 = 0x9b30ff;
pub(super) const PLUS_NAME: u32 = 0xd45af0;
pub(super) const GREEN: u32 = 0x4cc38a;
pub(super) const WHITE: u32 = 0xffffff;

pub(super) fn hue(color: u32) -> Hsla {
    rgb(color).into()
}

pub(super) fn gradient() -> Background {
    linear_gradient(
        90.,
        linear_color_stop(rgb(HOT_PINK), 0.),
        linear_color_stop(rgb(PURPLE), 1.),
    )
}

pub(super) fn diagonal() -> Background {
    linear_gradient(
        135.,
        linear_color_stop(rgb(HOT_PINK), 0.),
        linear_color_stop(rgb(PURPLE), 1.),
    )
}

/// The surface with a little of `color` mixed in.
pub(super) fn tinted(p: Palette, color: u32, amount: f32) -> Hsla {
    p.surface.blend(hue(color).opacity(amount))
}

pub(super) fn muted(text: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(text.into())
}

pub(super) fn notice(ok: bool, text: String, p: Palette) -> Div {
    div()
        .text_size(px(13.))
        .text_color(if ok { hue(GREEN) } else { p.error })
        .child(text)
}

pub(super) fn icon_tile(icon: IconName, size: f32, bg: impl Into<Fill>) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size * 0.28))
        .bg(bg)
        .child(Icon::new(icon).size(px(size * 0.48)).text_color(hue(WHITE)))
}

pub(super) fn chip(label: &'static str, bg: impl Into<Fill>, color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .px(px(10.))
        .py(px(3.))
        .rounded_full()
        .bg(bg)
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .whitespace_nowrap()
        .child(label.to_uppercase())
}

/// A soft white streak that sweeps left to right across its parent, which must be relative and clip.
pub(super) fn glint(id: impl Into<ElementId>, every: Duration) -> impl IntoElement {
    let half = |from: u32, to: u32| {
        div().h_full().w(px(9.)).bg(linear_gradient(
            90.,
            linear_color_stop(rgba(from), 0.),
            linear_color_stop(rgba(to), 1.),
        ))
    };
    h_flex()
        .absolute()
        .top_0()
        .bottom_0()
        .child(half(0xffffff00, 0xffffff99))
        .child(half(0xffffff99, 0xffffff00))
        .with_animation(id, Animation::new(every).repeat(), |el, t| {
            el.left(relative(t * 2.4 - 0.4))
        })
}

pub(super) fn badge(id: impl Into<ElementId>, plus: bool) -> Div {
    div()
        .relative()
        .overflow_hidden()
        .flex_shrink_0()
        .px(px(8.))
        .py(px(2.))
        .rounded_full()
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(hue(WHITE))
        .map(|d| match plus {
            true => d.bg(gradient()).child(glint(id, Duration::from_millis(3200))),
            false => d.bg(rgb(PINK)),
        })
        .child(tier_name(if plus { "supporter_plus" } else { "supporter" }))
}

pub(super) fn section_head(
    icon: IconName,
    title: &'static str,
    subtitle: &'static str,
    count: Option<usize>,
    p: Palette,
) -> Div {
    h_flex()
        .gap(px(14.))
        .child(icon_tile(icon, 44., diagonal()))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .child(
                            div()
                                .text_size(px(22.))
                                .font_weight(FontWeight::BOLD)
                                .child(title),
                        )
                        .children(count.map(|n| {
                            div()
                                .px(px(9.))
                                .py(px(1.))
                                .rounded_full()
                                .bg(p.elevated)
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.text_muted)
                                .child(n.to_string())
                        })),
                )
                .child(div().text_size(px(14.)).text_color(p.text_muted).child(subtitle)),
        )
}
