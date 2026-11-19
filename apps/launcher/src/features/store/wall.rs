use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::online::server::support::{BIO_MAX, Supporter, tier_name};
use swofty_i18n::tr;

use super::StorePage;
use super::parts::*;
use crate::features::time_trial::Load;
use crate::shell::Shell;
use crate::ui::Overlay;
use crate::ui::scale::px;
use crate::ui::theme::{Palette, ScrollArea};

const SUPPORTER_MIN: f32 = 220.;
const GAP: f32 = 10.;

fn empty_note(text: String, p: Palette) -> AnyElement {
    div()
        .w_full()
        .py(px(24.))
        .flex()
        .justify_center()
        .rounded(px(10.))
        .border_1()
        .border_color(p.border_variant)
        .bg(p.surface)
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(text)
        .into_any_element()
}

/// A quiet rule naming one tier's group on the wall and how many are in it.
fn group_label(plus: bool, count: usize, p: Palette) -> Div {
    h_flex()
        .h(px(16.))
        .gap(px(8.))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(hue(if plus { PLUS_NAME } else { PINK }))
                .child(tier_name(if plus { "supporter_plus" } else { "supporter" }).to_uppercase()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(11.))
                .text_color(p.text_placeholder)
                .child(count.to_string()),
        )
        .child(div().flex_1().h(px(1.)).bg(p.border_variant))
}

impl Shell {
    /// The supporter wall: it takes the rest of the page's height and scrolls on its own.
    pub(super) fn store_supporters(&self, p: Palette) -> AnyElement {
        let wall = match &self.store_page.supporters {
            Load::Idle | Load::Loading => empty_note(tr("Loading supporters…").to_owned(), p),
            Load::Failed(e) => empty_note(e.clone(), p),
            Load::Ready(s) if s.is_empty() => {
                empty_note(tr("No supporters yet. Be the first.").to_owned(), p)
            }
            Load::Ready(s) => {
                let width = self.layout.content_width;
                let cols = ((width + GAP) / (SUPPORTER_MIN + GAP)).floor().max(1.) as u16;
                let split = s.partition_point(Supporter::plus);
                let groups = [(true, 0, &s[..split]), (false, split, &s[split..])];
                v_flex()
                    .pr(px(4.))
                    .gap(px(16.))
                    .children(groups.into_iter().filter(|(_, _, g)| !g.is_empty()).map(
                        |(plus, from, group)| {
                            v_flex()
                                .gap(px(8.))
                                .child(group_label(plus, group.len(), p))
                                .child(
                                    div().w_full().grid().grid_cols(cols).gap(px(GAP)).children(
                                        group
                                            .iter()
                                            .enumerate()
                                            .map(|(i, x)| self.supporter_card(from + i, x, p)),
                                    ),
                                )
                        },
                    ))
                    .into_any_element()
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .scroll_y("store-supporters")
            .child(wall)
            .into_any_element()
    }

    fn supporter_card(&self, i: usize, s: &Supporter, p: Palette) -> AnyElement {
        let named = s.display.name.as_deref().filter(|_| !s.display.anonymous);
        let face = match named {
            Some(n) => self.face(n, s.display.avatar_url.as_deref(), 36., p),
            None => div()
                .size(px(36.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(p.elevated)
                .child(Icon::new(IconName::User).size(px(18.)).text_color(p.text_muted)),
        };
        let plus = s.plus();
        let face = face.when(plus, |d| d.border_2().border_color(rgb(PURPLE)));
        let bio = s.bio.clone().filter(|b| !b.trim().is_empty());
        h_flex()
            .id(SharedString::from(format!("supporter-{i}")))
            .min_w_0()
            .gap(px(10.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(10.))
            .border_1()
            .map(|d| match plus {
                true => d.border_color(hue(PURPLE).opacity(0.45)).bg(linear_gradient(
                    135.,
                    linear_color_stop(tinted(p, PURPLE, 0.16), 0.),
                    linear_color_stop(p.surface, 1.),
                )),
                false => d.border_color(p.border_variant).bg(p.surface),
            })
            .child(face)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(if plus { PLUS_NAME } else { PINK }))
                                    .child(named.map_or_else(|| tr("Anonymous").to_owned(), str::to_owned)),
                            )
                            .child(badge(SharedString::from(format!("supporter-glint-{i}")), plus)),
                    )
                    .children(bio.map(|b| {
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(b)
                    })),
            )
            .into_any_element()
    }

    /// The signed-in supporter's own line on the wall and how they are shown there, in a modal.
    pub(super) fn store_listing_modal(
        &self,
        bio: Entity<InputState>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Load::Ready(me) = &self.store_page.me else {
            return None;
        };
        if !me.active {
            return None;
        }
        let typed = bio.read(cx).value().to_string();
        let count = typed.chars().count();
        let mode = self.store_page.mode.clone();
        let busy = self.store_page.busy;
        let weak = cx.entity().downgrade();
        let choice = |id: &'static str, key: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .small()
                .when_else(mode == key, |b| b.primary(), |b| b.outline())
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store_page.mode = key.to_owned();
                    cx.notify();
                }))
        };
        let note = self
            .store_page
            .notice
            .as_ref()
            .map(|(ok, text)| notice(*ok, text.clone(), p));
        let panel = v_flex()
            .w(px(440.))
            .gap(px(12.))
            .p(px(20.))
            .rounded(px(12.))
            .border_1()
            .border_color(p.border)
            .bg(p.background)
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::BOLD)
                    .child(tr("Your listing")),
            )
            .child(muted(tr("Your line next to your name in the supporter list"), p))
            .child(Input::new(&bio).small())
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(muted(tr("Show me as"), p))
                    .child(choice("mode-discord", "discord", tr("Discord name & avatar"), cx))
                    .child(choice("mode-anon", "anonymous", tr("Anonymous"), cx)),
            )
            .children(note)
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(if count > BIO_MAX { p.error } else { p.text_muted })
                            .child(format!("{count} / {BIO_MAX}")),
                    )
                    .child(
                        Button::new("store-listing-close")
                            .small()
                            .ghost()
                            .label(tr("Close"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.store_page.listing_open = false;
                                this.store_page.notice = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("store-save")
                            .small()
                            .primary()
                            .label(tr("Save"))
                            .disabled(busy || count > BIO_MAX)
                            .on_click(
                                cx.listener(move |this, _, _, cx| StorePage::save(this, typed.clone(), cx)),
                            ),
                    ),
            );
        Some(
            Overlay::new("store-listing-modal")
                .on_dismiss(move |_, cx| {
                    weak.update(cx, |this, cx| {
                        this.store_page.listing_open = false;
                        cx.notify();
                    })
                    .ok();
                })
                .child(panel)
                .into_any_element(),
        )
    }
}
