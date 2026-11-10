use super::images::Art;
use super::model::*;
use super::{Online, Target};
use crate::scale::{Hit, px};
use crate::shell::Shell;
use crate::theme::Palette;
use crate::ui::{Callout, EmptyState};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::tr;

pub(super) fn tone(seed: &str) -> Hsla {
    let mut h: u32 = 2166136261;
    for b in seed.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16777619);
    }
    hsla((h % 360) as f32 / 360., 0.42, 0.34, 1.)
}

pub(super) fn medal(rank: u32, p: &Palette) -> Hsla {
    match rank {
        1 => hsla(0.125, 0.88, 0.58, 1.),
        2 => hsla(0.6, 0.12, 0.76, 1.),
        3 => hsla(0.07, 0.55, 0.55, 1.),
        _ => p.text_muted,
    }
}

pub(super) fn panel(p: Palette) -> Div {
    v_flex()
        .rounded(px(12.))
        .border_1()
        .border_color(p.border_variant)
        .bg(p.surface)
        .overflow_hidden()
}

pub(super) fn section_title(text: &'static str, p: Palette) -> Div {
    div()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text_muted)
        .child(text)
}

pub(super) fn chip(label: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .px(px(7.))
        .py(px(1.))
        .rounded(px(5.))
        .bg(p.elevated)
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text_muted)
        .child(label.into())
}

impl Online {
    pub(super) fn ringed(&self, index: usize) -> bool {
        self.cursor == Some(index)
    }
}

impl Shell {
    pub(super) fn online_art(&self, url: Option<&String>) -> Option<&Art> {
        url.and_then(|u| self.online.images.get(u))
    }

    pub(super) fn tt_cover(&self, url: Option<&String>, seed: &str) -> Div {
        let base = div().size_full().bg(tone(seed));
        match self.online_art(url) {
            Some(Art::Raster(image)) => {
                base.child(img(image.clone()).size_full().object_fit(ObjectFit::Cover))
            }
            _ => base.bg(linear_gradient(
                135.,
                linear_color_stop(tone(seed), 0.),
                linear_color_stop(tone(&format!("{seed}-b")), 1.),
            )),
        }
    }

    pub(super) fn tt_face(&self, d: &Driver, size: f32, p: Palette) -> Div {
        let frame = div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden()
            .border_1()
            .border_color(p.surface);
        match self.online_art(d.avatar.as_ref()) {
            Some(Art::Raster(image)) => {
                frame.child(img(image.clone()).size_full().object_fit(ObjectFit::Cover))
            }
            _ => frame
                .bg(tone(&d.name))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(size * 0.42))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0xffffff))
                .child(
                    d.name
                        .chars()
                        .take(1)
                        .flat_map(char::to_uppercase)
                        .collect::<String>(),
                ),
        }
    }

    pub(super) fn tt_faces(&self, drivers: &[Driver], size: f32, p: Palette) -> Div {
        h_flex().children(
            drivers
                .iter()
                .enumerate()
                .map(|(i, d)| self.tt_face(d, size, p).when(i > 0, |f| f.ml(px(-size * 0.3)))),
        )
    }

    pub(super) fn layout_art(&self, url: Option<&String>, size: f32, color: Hsla, p: Palette) -> Div {
        let frame = div()
            .size(px(size))
            .flex_shrink_0()
            .rounded(px(10.))
            .bg(p.elevated)
            .flex()
            .items_center()
            .justify_center();
        match self.online_art(url) {
            Some(Art::Svg(data)) => frame.child(svg().data(data).size(px(size * 0.78)).text_color(color)),
            _ => frame.child(
                Icon::new(IconName::Route)
                    .size(px(size * 0.35))
                    .text_color(p.text_placeholder),
            ),
        }
    }

    pub(super) fn flag_chip(&self, code: &Option<String>, p: Palette) -> Option<Div> {
        code.as_ref().map(|c| chip(c.to_uppercase(), p))
    }

    pub(super) fn crumb_bar(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let crumbs: Vec<(usize, String)> = self
            .online
            .trail
            .crumbs()
            .into_iter()
            .map(|(d, l)| (d, l.to_owned()))
            .collect();
        let last = crumbs.len();
        h_flex()
            .px(px(28.))
            .pt(px(18.))
            .pb(px(10.))
            .gap(px(8.))
            .items_center()
            .text_size(px(13.))
            .child(
                Button::new("trail-back")
                    .ghost()
                    .tap_sm()
                    .icon(IconName::ChevronLeft)
                    .disabled(self.online.trail.depth() <= 1)
                    .on_click(cx.listener(|this, _, _, cx| this.online_back(cx))),
            )
            .child(
                div()
                    .id("crumb-root")
                    .cursor_pointer()
                    .text_color(if last == 0 { p.text } else { p.text_muted })
                    .font_weight(if last == 0 {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .hover(move |s| s.text_color(p.text))
                    .child(tr("Time Trial"))
                    .on_click(cx.listener(|this, _, _, cx| this.online_jump(1, cx))),
            )
            .children(crumbs.into_iter().enumerate().flat_map(|(i, (depth, label))| {
                let current = i + 1 == last;
                [
                    Icon::new(IconName::ChevronRight)
                        .size(px(13.))
                        .text_color(p.text_placeholder)
                        .into_any_element(),
                    div()
                        .id(SharedString::from(format!("crumb-{depth}")))
                        .cursor_pointer()
                        .text_color(if current { p.text } else { p.text_muted })
                        .font_weight(if current {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .hover(move |s| s.text_color(p.text))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| this.online_jump(depth, cx)))
                        .into_any_element(),
                ]
            }))
            .child(div().flex_1())
            .child(
                Button::new("online-refresh")
                    .ghost()
                    .tap_sm()
                    .icon(IconName::RefreshCw)
                    .label(tr("Refresh"))
                    .on_click(cx.listener(|this, _, _, cx| this.online_reload(cx))),
            )
            .into_any_element()
    }

    pub(super) fn sign_in_banner(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.account_name()
            .is_none()
            .then(|| {
                div()
                    .px(px(28.))
                    .pb(px(8.))
                    .child(
                        Callout::info(tr("Log in with Discord to see your own place on the boards."))
                            .icon(IconName::User)
                            .actions(
                                h_flex().child(
                                    Button::new("online-login")
                                        .outline()
                                        .tap_sm()
                                        .label(tr("Log in with Discord"))
                                        .on_click(
                                            cx.listener(|this, _, window, cx| this.mp_sign_in(window, cx)),
                                        ),
                                ),
                            ),
                    )
                    .into_any_element()
            })
            .filter(|_| {
                let _ = p;
                true
            })
    }

    pub(super) fn skeleton(&self, p: Palette, rows: usize, height: f32) -> AnyElement {
        v_flex()
            .px(px(28.))
            .gap(px(12.))
            .children((0..rows).map(|_| {
                div()
                    .h(px(height))
                    .rounded(px(12.))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border_variant)
            }))
            .into_any_element()
    }

    pub(super) fn failed_view(&self, e: &OnlineError, cx: &mut Context<Self>) -> AnyElement {
        EmptyState::new(IconName::CloudOff, tr("Can't load the boards"))
            .body(e.message.clone())
            .action(
                Button::new("online-retry")
                    .outline()
                    .tap_sm()
                    .icon(IconName::RefreshCw)
                    .label(tr("Try again"))
                    .on_click(cx.listener(|this, _, _, cx| this.online_reload(cx))),
            )
            .into_any_element()
    }

    pub(super) fn nothing_yet(&self) -> AnyElement {
        EmptyState::new(IconName::Trophy, tr("No laps yet"))
            .body(tr("Time Trial laps show up here once they are uploaded."))
            .into_any_element()
    }

    pub(super) fn target_item(&mut self, items: &mut Vec<Target>, t: Target) -> usize {
        items.push(t);
        items.len() - 1
    }
}
