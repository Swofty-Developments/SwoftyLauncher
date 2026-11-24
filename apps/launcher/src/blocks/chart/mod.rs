//! Native GPU chart marks, text and optional supplied pictures. No generated charts or webview.

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use swofty_core::view::*;
use swofty_i18n::{tr, trf};

use crate::ui::scale::px;
use crate::ui::theme::Palette;

mod appearance;
mod images;
#[cfg(test)]
mod options_tests;
mod paint;
mod scene;
#[cfg(test)]
mod tests;

pub(crate) use images::DemoImages;
pub(crate) use images::Pictures;
use paint::paint;
pub(crate) use scene::Scene;

pub(super) fn resolve_images(
    c: &Chart,
    cache: &std::cell::RefCell<super::image_data::Images>,
    roots: &super::select::ImageRoots,
) -> Pictures {
    images::resolve(c, cache, Some(roots))
}

const COLORS: [u32; 10] = [
    0x57b7ff, 0xffa52f, 0x28cbb2, 0xf26787, 0xb99aff, 0xffdc72, 0x77c65e, 0xee9be0, 0x99aaba, 0xe18d60,
];

fn color(s: Option<&str>, fallback: Hsla) -> Hsla {
    s.and_then(|s| {
        u32::from_str_radix(&s[1..], 16).ok().map(|n| {
            if s.len() == 9 {
                let mut c: Hsla = rgb(n >> 8).into();
                c.a = (n & 255) as f32 / 255.;
                c
            } else {
                rgb(n).into()
            }
        })
    })
    .unwrap_or(fallback)
}

/// Shared by addon blocks and the standalone chart gallery; IDs are scoped by caller.
pub(crate) fn prepare(c: &Chart) -> Arc<Scene> {
    Arc::new(Scene::new(c))
}

pub(crate) fn render(
    id: String,
    c: &Chart,
    p: Palette,
    prepared: Option<Arc<Scene>>,
    pictures: Pictures,
) -> AnyElement {
    let mut root = v_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .gap(px(8.))
        .p(px(16.))
        .rounded(px(10.))
        .border_1()
        .border_color(p.border_variant)
        .bg(p.surface)
        .text_color(p.text);
    let error = c.validate().err();
    let options = error.is_none().then_some(c.options.as_deref()).flatten();
    if let Some(o) = options {
        if let Some(background) = &o.background {
            root = root.bg(color(Some(background), p.surface));
        }
        if let Some(border) = &o.border {
            root = root
                .border(px(border.width))
                .border_color(color(Some(&border.color), p.border_variant));
        }
        if let Some(radius) = o.corner_radius {
            root = root.rounded(px(radius));
        }
    }
    if !c.title.is_empty() {
        root = root.child(
            div()
                .text_size(px(options.and_then(|o| o.text.title).unwrap_or(14.)))
                .font_weight(FontWeight::SEMIBOLD)
                .child(c.title.clone()),
        );
    }
    if let Some(error) = error {
        return root
            .child(
                div()
                    .text_color(p.error)
                    .child(trf!("Cannot display chart: {error}", error = error)),
            )
            .into_any_element();
    }
    let scene = prepared.unwrap_or_else(|| prepare(c));
    if !scene.has_data {
        return root
            .child(
                div()
                    .h(px(c.height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.text_muted)
                    .child(tr("No chart data")),
            )
            .into_any_element();
    }
    let appearance = appearance::Appearance::new(c, pictures);
    let legend = h_flex().flex_wrap().gap(px(14.)).children(
        c.series
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.name.is_empty())
            .map(|(i, s)| {
                let picture = appearance.picture(&ChartImageLocation::Legend { series: i });
                h_flex()
                    .gap(px(5.))
                    .child(picture.map(|i| i.element(p)).unwrap_or_else(|| {
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(color(s.color.as_deref(), rgb(COLORS[i % COLORS.len()]).into()))
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .text_size(px(options.and_then(|o| o.text.legend).unwrap_or(11.)))
                            .child(s.name.clone()),
                    )
            }),
    );
    let axes = [c.x.clone(), c.y.clone()];
    root = root.child(legend).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                paint(&scene, &axes, &appearance, bounds, p, window, cx);
            },
        )
        .w_full()
        .flex_shrink_0()
        .h(px(c.height))
        .min_w(px(100.)),
    );
    if !c.caption.is_empty() {
        root = root.child(
            div()
                .text_size(px(options.and_then(|o| o.text.caption).unwrap_or(11.)))
                .text_color(p.text_muted)
                .child(c.caption.clone()),
        );
    }
    root.into_any_element()
}
