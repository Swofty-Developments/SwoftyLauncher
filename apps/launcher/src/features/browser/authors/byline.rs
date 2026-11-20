//! A publisher's avatar and name where an addon names them, and the bar back along the trail.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::catalog::Addon;
use swofty_i18n::{tr, trf};

use super::{CRUMBS_SHOWN, View, face_color};
use crate::shell::Shell;
use crate::ui::back_row;
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;
use crate::ui::tokens::space::GUTTER;

impl Shell {
    pub(in crate::features::browser) fn publisher_avatar(
        &self,
        id: &str,
        name: &str,
        url: Option<&str>,
        size: f32,
        p: Palette,
    ) -> Div {
        let face = div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden();
        match url.and_then(|u| self.browser.avatars.get(u)) {
            Some(image) => face.child(img(image.clone()).size_full().rounded_full()),
            None => {
                let c = face_color(id, p);
                let letter: String = name.trim().chars().take(1).flat_map(char::to_uppercase).collect();
                face.flex()
                    .items_center()
                    .justify_center()
                    .bg(c.opacity(0.16))
                    .border_1()
                    .border_color(c.opacity(0.45))
                    .text_color(c)
                    .text_size(px(size * 0.44))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(letter)
            }
        }
    }

    pub(super) fn author_link(
        &self,
        key: String,
        id: &str,
        name: &str,
        size: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let target = id.to_owned();
        div()
            .id(SharedString::from(key))
            .min_w_0()
            .truncate()
            .cursor_pointer()
            .text_size(px(size))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.text)
            .hover(move |s| s.text_color(p.accent).underline())
            .child(name.to_owned())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.browse(View::Author(target.clone()), cx);
            }))
    }

    /// "by <publisher>", the publisher a link to their page, with the
    /// manifest's author after it when that's someone else.
    pub(in crate::features::browser) fn byline(
        &self,
        a: &Addon,
        key: String,
        size: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let row = h_flex()
            .min_w_0()
            .gap(px(4.))
            .text_size(px(size))
            .text_color(p.text_muted)
            .whitespace_nowrap()
            .overflow_hidden();
        let Some(owner) = a.owner() else {
            return row.child(div().truncate().child(match a.author_name() {
                "" => a.id.clone(),
                name => trf!("by {name}", name = name),
            }));
        };
        let credit = a.credit().map(|credit| {
            let tip = trf!(
                "{name} published it. Its manifest credits {credit}.",
                name = owner.name,
                credit = credit
            );
            div()
                .id(SharedString::from(format!("{key}-credit")))
                .min_w_0()
                .truncate()
                .text_color(p.text_placeholder)
                .child(format!("· {}", trf!("credited to {credit}", credit = credit)))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        });
        row.child(div().flex_shrink_0().child(tr("by")))
            .child(
                self.author_link(key, &owner.id, &owner.name, size, p, cx)
                    .flex_shrink_0()
                    .max_w(relative(0.75)),
            )
            .children(credit)
    }

    /// The Back button and the trail, above the part of the page that scrolls.
    pub(in crate::features::browser) fn back_bar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let mut crumbs: Vec<(Option<usize>, String)> = b
            .trail
            .steps()
            .iter()
            .enumerate()
            .map(|(i, (v, _))| (Some(i), b.view_label(v)))
            .collect();
        crumbs.push((None, b.view_label(b.trail.view())));
        let cut = crumbs.len().saturating_sub(CRUMBS_SHOWN);
        let mut parts: Vec<AnyElement> = Vec::new();
        if cut > 0 {
            parts.push(div().child("…").into_any_element());
        }
        for (n, (at, text)) in crumbs.into_iter().skip(cut).enumerate() {
            if n > 0 || cut > 0 {
                parts.push(
                    Icon::new(IconName::ChevronRight)
                        .size(px(12.))
                        .text_color(p.text_placeholder)
                        .into_any_element(),
                );
            }
            parts.push(match at {
                Some(i) => div()
                    .id(SharedString::from(format!("browser-crumb-{i}")))
                    .max_w(px(220.))
                    .truncate()
                    .cursor_pointer()
                    .hover(move |s| s.text_color(p.text).underline())
                    .child(text)
                    .on_click(cx.listener(move |this, _, _, cx| this.browse_back_to(i, cx)))
                    .into_any_element(),
                None => div()
                    .max_w(px(260.))
                    .truncate()
                    .text_color(p.text)
                    .font_weight(FontWeight::MEDIUM)
                    .child(text)
                    .into_any_element(),
            });
        }
        back_row(self.page_back_button(cx), parts, p)
            .px(px(GUTTER))
            .pb(px(12.))
            .border_b_1()
            .border_color(p.border_variant)
    }

    pub(in crate::features::browser) fn creators_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("browser-creators")
            .map(|b| {
                if *self.browser.trail.view() == View::Creators {
                    b.outline()
                } else {
                    b.ghost()
                }
            })
            .tap_sm()
            .icon(IconName::Users)
            .label(tr("Creators"))
            .on_click(cx.listener(|this, _, _, cx| this.browse_creators(cx)))
    }
}
