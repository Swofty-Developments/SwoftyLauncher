use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::Duration;
use swofty_core::online::server::support::{Kind, format_date, tier_name};
use swofty_i18n::{tr, trf};

use super::StorePage;
use super::parts::*;
use crate::features::time_trial::Load;
use crate::shell::Shell;
use crate::ui::scale::px;
use crate::ui::theme::{Palette, ScrollArea};
use crate::ui::tokens::space::GUTTER;

fn perk_row(icon: IconName, text: SharedString, accent: Hsla) -> Div {
    h_flex()
        .gap(px(14.))
        .child(
            div()
                .size(px(34.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(accent.opacity(0.16))
                .child(Icon::new(icon).size(px(17.)).text_color(accent)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(16.))
                .line_height(px(22.))
                .child(text),
        )
}

impl Shell {
    pub(crate) fn store_page(
        &mut self,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (bio, amount, email, payment) = StorePage::inputs(self, window, cx);
        if !self.store_page.bio_synced
            && let Load::Ready(me) = &self.store_page.me
        {
            let text = me.bio.clone().unwrap_or_default();
            bio.update(cx, |s, cx| s.set_value(text, window, cx));
            self.store_page.bio_synced = true;
        }
        let width = self.layout.content_width.min(MAX_WIDTH);
        let modal = self
            .store_page
            .claim_open
            .then(|| self.store_claim_modal(email, payment, p, cx));
        let note = self
            .store_page
            .notice
            .as_ref()
            .filter(|_| !self.store_page.claim_open)
            .map(|(ok, text)| notice(*ok, text.clone(), p));
        let body = v_flex()
            .w_full()
            .max_w(px(MAX_WIDTH))
            .gap(px(44.))
            .child(self.store_hero(p, cx))
            .child(
                v_flex()
                    .gap(px(16.))
                    .child(self.store_tiers(width, p, cx))
                    .child(self.store_onetime(amount, width, p, cx))
                    .children(note),
            )
            .children(self.store_previews(width, p))
            .child(self.store_supporters(bio, width, p, cx));
        v_flex()
            .relative()
            .size_full()
            .child(
                div().flex_1().min_h_0().scroll_y("store-page").child(
                    h_flex()
                        .w_full()
                        .items_start()
                        .justify_center()
                        .px(px(GUTTER))
                        .pt(px(20.))
                        .pb(px(44.))
                        .child(body),
                ),
            )
            .children(modal)
            .into_any_element()
    }

    fn store_hero(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let refresh = Button::new("store-refresh")
            .ghost()
            .icon(IconName::RefreshCw)
            .on_click(cx.listener(|this, _, _, cx| StorePage::reload(this, cx)));
        let claim = Button::new("store-claim-open")
            .outline()
            .label(tr("Claim previous donation"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.store_page.claim_open = true;
                this.store_page.notice = None;
                cx.notify();
            }));
        let error = self
            .library
            .error
            .clone()
            .map(|e| div().text_size(px(13.)).text_color(p.error).child(e));
        div()
            .relative()
            .overflow_hidden()
            .w_full()
            .rounded(px(18.))
            .border_1()
            .border_color(hue(HOT_PINK).opacity(0.3))
            .bg(linear_gradient(
                115.,
                linear_color_stop(tinted(p, HOT_PINK, 0.2), 0.),
                linear_color_stop(tinted(p, PURPLE, 0.12), 1.),
            ))
            .child(
                Icon::new(IconName::ShoppingBag)
                    .absolute()
                    .top(px(-36.))
                    .right(px(160.))
                    .size(px(210.))
                    .text_color(hue(WHITE).opacity(0.04)),
            )
            .child(
                h_flex()
                    .relative()
                    .flex_wrap()
                    .gap(px(24.))
                    .px(px(32.))
                    .py(px(30.))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w(px(280.))
                            .gap(px(20.))
                            .child(icon_tile(IconName::ShoppingBag, 64., diagonal()))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(34.))
                                            .line_height(px(38.))
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .child(tr("Store")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(15.))
                                            .text_color(p.text_muted)
                                            .child(tr("Keep Swofty Launcher free and open source")),
                                    )
                                    .children(self.store_status(p))
                                    .children(error),
                            ),
                    )
                    .child(h_flex().gap(px(8.)).child(claim).child(refresh)),
            )
            .into_any_element()
    }

    /// The signed-in supporter's tier and time left, or what to do to subscribe when signed out.
    fn store_status(&self, p: Palette) -> Option<AnyElement> {
        if self.account.session.is_none() {
            return Some(
                muted(tr("Sign in with Discord to subscribe."), p)
                    .pt(px(6.))
                    .into_any_element(),
            );
        }
        let Load::Ready(me) = &self.store_page.me else {
            return None;
        };
        if !me.active {
            return None;
        }
        let plus = me.tier.as_deref() == Some("supporter_plus");
        let when = me.expires_at.as_ref().map(format_date).unwrap_or_default();
        Some(
            h_flex()
                .pt(px(8.))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .pl(px(6.))
                        .pr(px(14.))
                        .py(px(5.))
                        .rounded_full()
                        .bg(hue(WHITE).opacity(0.06))
                        .child(badge("store-me-glint", plus))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(trf!(
                                    "{months} months left (expires {date})",
                                    months = me.months_left.unwrap_or(0),
                                    date = when
                                )),
                        ),
                )
                .into_any_element(),
        )
    }

    fn store_tiers(&self, width: f32, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let current = match &self.store_page.me {
            Load::Ready(me) if me.active => me.tier.as_deref(),
            _ => None,
        };
        let basic: Vec<(IconName, SharedString)> = vec![
            (IconName::Ban, tr("No ads in the launcher").into()),
            (IconName::Eye, tr("Access to #f1-supporter-leaks").into()),
            (
                IconName::MessageCircle,
                tr("The #f1-supporter-general chat").into(),
            ),
            (
                IconName::Sticker,
                tr("Discord embed links and external emoji permissions").into(),
            ),
            (
                IconName::Heart,
                tr("Supporting a free and open-source launcher").into(),
            ),
        ];
        let mut plus: Vec<(IconName, SharedString)> = vec![
            (IconName::BadgeCheck, tr("All of Supporter").into()),
            (IconName::FlaskConical, tr("Access to private betatesting").into()),
        ];
        plus.extend(
            self.store_page
                .previews
                .iter()
                .filter(|x| !x.title.trim().is_empty())
                .take(3)
                .map(|x| {
                    (
                        IconName::Sparkles,
                        trf!("Early access to {title}", title = x.title.trim()).into(),
                    )
                }),
        );
        let cards = [
            self.tier_card(Kind::Supporter, "$5", basic, current == Some("supporter"), p, cx),
            self.tier_card(
                Kind::SupporterPlus,
                "$20",
                plus,
                current == Some("supporter_plus"),
                p,
                cx,
            ),
        ];
        div()
            .w_full()
            .grid()
            .grid_cols(if width >= SIDE_BY_SIDE { 2 } else { 1 })
            .gap(px(20.))
            .children(cards)
            .into_any_element()
    }

    fn tier_card(
        &self,
        kind: Kind,
        price: &'static str,
        perks: Vec<(IconName, SharedString)>,
        current: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let plus = kind == Kind::SupporterPlus;
        let accent = hue(if plus { PLUS_NAME } else { PINK });
        let tagline = if plus {
            tr("Everything in Supporter, plus early access to addons in testing")
        } else {
            tr("Ad-free launcher and the supporter channels in Discord")
        };
        let head = h_flex()
            .gap(px(14.))
            .child(icon_tile(
                if plus { IconName::Crown } else { IconName::Heart },
                46.,
                if plus { diagonal() } else { hue(PINK).into() },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(22.))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .text_color(accent)
                    .child(tier_name(if plus { "supporter_plus" } else { "supporter" })),
            )
            .child(
                h_flex()
                    .gap(px(6.))
                    .when(current, |d| {
                        d.child(chip(tr("Your tier"), hue(GREEN).opacity(0.16), hue(GREEN)))
                    })
                    .when(plus, |d| {
                        d.child(chip(tr("Early access"), gradient(), hue(WHITE)))
                    }),
            );
        let price = h_flex()
            .items_end()
            .gap(px(6.))
            .pt(px(24.))
            .child(
                div()
                    .text_size(px(50.))
                    .line_height(px(52.))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .child(price),
            )
            .child(
                div()
                    .pb(px(8.))
                    .text_size(px(17.))
                    .text_color(p.text_muted)
                    .child(tr("/ month")),
            );
        let card = v_flex()
            .flex_1()
            .p(px(30.))
            .map(|d| match plus {
                true => d.rounded(px(16.)).bg(linear_gradient(
                    180.,
                    linear_color_stop(tinted(p, PURPLE, 0.18), 0.),
                    linear_color_stop(p.surface, 0.75),
                )),
                false => d.rounded(px(18.)).border_1().border_color(p.border).bg(p.surface),
            })
            .child(head)
            .child(price)
            .child(muted(tagline, p).text_size(px(14.)).pt(px(6.)))
            .child(div().my(px(24.)).h(px(1.)).w_full().bg(p.border_variant))
            .child(
                v_flex()
                    .gap(px(16.))
                    .children(perks.into_iter().map(|(icon, text)| perk_row(icon, text, accent))),
            )
            .child(div().flex_1().min_h(px(28.)))
            .child(self.subscribe_button(kind, cx));
        match plus {
            true => v_flex()
                .p(px(2.))
                .rounded(px(18.))
                .bg(diagonal())
                .shadow_lg()
                .child(card)
                .into_any_element(),
            false => v_flex().child(card).into_any_element(),
        }
    }

    fn subscribe_button(&self, kind: Kind, cx: &mut Context<Self>) -> Stateful<Div> {
        let plus = kind == Kind::SupporterPlus;
        let busy = self.store_page.busy;
        let signed_in = self.account.session.is_some();
        div()
            .id(if plus {
                "store-subscribe-plus"
            } else {
                "store-subscribe"
            })
            .relative()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .h(px(52.))
            .w_full()
            .rounded(px(12.))
            .text_size(px(16.))
            .font_weight(FontWeight::EXTRA_BOLD)
            .text_color(hue(WHITE))
            .map(|d| match plus {
                true => d
                    .bg(gradient())
                    .child(glint("store-plus-glint", Duration::from_millis(2600))),
                false => d.bg(rgb(PINK)),
            })
            .when(busy, |d| d.opacity(0.6))
            .when(!busy, |d| d.cursor_pointer().hover(|d| d.opacity(0.9)))
            .child(tr("Subscribe"))
            .child(
                Icon::new(IconName::ArrowRight)
                    .size(px(18.))
                    .text_color(hue(WHITE)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if signed_in {
                    StorePage::checkout(this, kind, None, cx);
                } else {
                    this.store_page.notice =
                        Some((false, tr("Sign in with Discord to subscribe.").to_owned()));
                    cx.notify();
                }
            }))
    }

    fn store_onetime(
        &self,
        amount: Entity<InputState>,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.store_page.busy;
        let typed = amount.read(cx).value().to_string();
        let donate = div()
            .id("store-onetime")
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(36.))
            .px(px(18.))
            .rounded(px(10.))
            .bg(rgb(YELLOW))
            .text_color(rgb(0x1a1400))
            .text_size(px(14.))
            .font_weight(FontWeight::BOLD)
            .when(busy, |d| d.opacity(0.6))
            .when(!busy, |d| d.cursor_pointer().hover(|d| d.opacity(0.9)))
            .child(tr("Donate"))
            .on_click(cx.listener(move |this, _, _, cx| {
                StorePage::checkout(this, Kind::OneTime, Some(typed.clone()), cx)
            }));
        let form = h_flex()
            .gap(px(10.))
            .child(
                div().w(px(150.)).child(
                    Input::new(&amount)
                        .small()
                        .prefix(div().text_color(p.text_muted).child("$")),
                ),
            )
            .child(donate);
        let about = h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(14.))
            .child(icon_tile(IconName::Gift, 40., hue(YELLOW).opacity(0.18)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("One-time donation")),
                    )
                    .child(muted(tr("Converts to whole months of a tier"), p)),
            );
        let row = match width >= SIDE_BY_SIDE {
            true => h_flex().gap(px(16.)).child(about).child(form),
            false => v_flex().gap(px(12.)).child(about).child(form),
        };
        row.px(px(20.))
            .py(px(14.))
            .rounded(px(14.))
            .border_1()
            .border_color(p.border_variant)
            .bg(p.surface)
            .into_any_element()
    }
}
