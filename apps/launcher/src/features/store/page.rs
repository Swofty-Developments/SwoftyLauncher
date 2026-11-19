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
use crate::ui::theme::Palette;
use crate::ui::tokens::space::GUTTER;

/// The share of the page's height the tiers and early access row starts from; it grows when the tiers need more.
const TOP_SHARE: f32 = 0.55;
const ROW_GAP: f32 = 16.;
const TIERS_MAX: f32 = 860.;

fn perk_row(text: &'static str, accent: Hsla, p: Palette) -> Div {
    h_flex()
        .items_start()
        .gap(px(8.))
        .child(
            Icon::new(IconName::Check)
                .size(px(14.))
                .mt(px(2.))
                .flex_shrink_0()
                .text_color(accent),
        )
        .child(
            div()
                .min_w_0()
                .text_size(px(13.))
                .line_height(px(18.))
                .text_color(p.text)
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
        let claim = self
            .store_page
            .claim_open
            .then(|| self.store_claim_modal(email, payment, p, cx));
        let listing = self
            .store_page
            .listing_open
            .then(|| self.store_listing_modal(bio, p, cx))
            .flatten();
        let tiers = self.store_tiers(amount, p, cx);
        let top = h_flex()
            .flex_basis(relative(TOP_SHARE))
            .flex_shrink_0()
            .items_stretch()
            .gap(px(ROW_GAP))
            .child(tiers)
            .children(self.store_previews(p));
        v_flex()
            .relative()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .px(px(GUTTER))
                    .pt(px(12.))
                    .pb(px(16.))
                    .gap(px(ROW_GAP))
                    .child(top)
                    .child(self.store_supporters(p, cx)),
            )
            .children(claim)
            .children(listing)
            .into_any_element()
    }

    /// The signed-in supporter's tier and time left, or what to do to subscribe, then the page's actions.
    fn store_status(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let refresh = Button::new("store-refresh")
            .ghost()
            .small()
            .icon(IconName::RefreshCw)
            .on_click(cx.listener(|this, _, _, cx| StorePage::reload(this, cx)));
        let claim = Button::new("store-claim-open")
            .ghost()
            .small()
            .label(tr("Claim previous donation"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.store_page.claim_open = true;
                this.store_page.notice = None;
                cx.notify();
            }));
        let status: Option<AnyElement> = match &self.store_page.me {
            _ if self.account.session.is_none() => Some(
                muted(tr("Sign in with Discord to subscribe."), p)
                    .text_size(px(12.))
                    .into_any_element(),
            ),
            Load::Ready(me) if me.active => {
                let plus = me.tier.as_deref() == Some("supporter_plus");
                let when = me.expires_at.as_ref().map(format_date).unwrap_or_default();
                Some(
                    h_flex()
                        .flex_shrink_0()
                        .gap(px(8.))
                        .child(badge("store-me-glint", plus))
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(trf!(
                                    "{months} months left (expires {date})",
                                    months = me.months_left.unwrap_or(0),
                                    date = when
                                )),
                        )
                        .into_any_element(),
                )
            }
            _ => None,
        };
        let note = self
            .store_page
            .notice
            .as_ref()
            .filter(|_| !self.store_page.claim_open && !self.store_page.listing_open)
            .map(|(ok, text)| notice(*ok, text.clone(), p).text_size(px(12.)).truncate());
        h_flex()
            .flex_shrink_0()
            .max_w_full()
            .h(px(26.))
            .justify_center()
            .gap(px(10.))
            .children(status)
            .children(note.map(|n| n.min_w_0().max_w(px(360.))))
            .child(claim)
            .child(refresh)
    }

    fn store_tiers(&self, amount: Entity<InputState>, p: Palette, cx: &mut Context<Self>) -> Div {
        let current = match &self.store_page.me {
            Load::Ready(me) if me.active => me.tier.as_deref(),
            _ => None,
        };
        let basic = vec![
            tr("No ads in the launcher"),
            tr("Access to #f1-supporter-leaks"),
            tr("The #f1-supporter-general chat"),
            tr("Discord embed links and external emoji permissions"),
            tr("Supporting a free and open-source launcher"),
        ];
        let plus = vec![tr("All of Supporter"), tr("Access to private betatesting")];
        let columns = h_flex()
            .flex_1()
            .w_full()
            .max_w(px(TIERS_MAX))
            .items_stretch()
            .child(self.tier_column(Kind::Supporter, basic, current == Some("supporter"), p, cx))
            .child(div().flex_shrink_0().w(px(1.)).my(px(10.)).bg(p.border_variant))
            .child(self.tier_column(
                Kind::SupporterPlus,
                plus,
                current == Some("supporter_plus"),
                p,
                cx,
            ));
        v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(10.))
            .child(self.store_status(p, cx))
            .child(columns)
            .child(self.store_onetime(amount, p, cx))
    }

    /// One tier as a centred column: its name, price, pitch and perks over its Subscribe button.
    fn tier_column(
        &self,
        kind: Kind,
        perks: Vec<&'static str>,
        current: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let plus = kind == Kind::SupporterPlus;
        let (price, tagline) = match plus {
            true => ("$20", tr("Play new addons before anyone else")),
            false => ("$5", tr("Back the launcher and lose the ads")),
        };
        let accent = hue(if plus { PLUS_NAME } else { PINK });
        let chips = h_flex()
            .h(px(20.))
            .justify_center()
            .gap(px(6.))
            .when(plus, |d| {
                d.child(chip(tr("Early access"), gradient(), hue(WHITE)))
            })
            .when(current, |d| {
                d.child(chip(tr("Your tier"), hue(GREEN).opacity(0.16), hue(GREEN)))
            });
        let name = h_flex()
            .gap(px(6.))
            .child(
                Icon::new(if plus { IconName::Crown } else { IconName::Heart })
                    .size(px(15.))
                    .text_color(accent),
            )
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(accent)
                    .child(tier_name(if plus { "supporter_plus" } else { "supporter" })),
            );
        let price = h_flex()
            .items_end()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(40.))
                    .line_height(px(42.))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .when(plus, |d| d.text_color(accent))
                    .child(price),
            )
            .child(
                div()
                    .pb(px(7.))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(tr("/ month")),
            );
        v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .px(px(20.))
            .child(chips)
            .child(name.mt(px(6.)))
            .child(price)
            .child(
                div()
                    .max_w(px(280.))
                    .text_center()
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(p.text_muted)
                    .child(tagline),
            )
            .child(
                v_flex()
                    .max_w_full()
                    .mt(px(12.))
                    .gap(px(5.))
                    .children(perks.into_iter().map(|text| perk_row(text, accent, p))),
            )
            .child(div().flex_1().min_h(px(12.)))
            .child(self.subscribe_button(kind, cx))
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
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .h(px(34.))
            .w_full()
            .max_w(px(220.))
            .rounded(px(8.))
            .text_size(px(14.))
            .font_weight(FontWeight::BOLD)
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
                    .size(px(15.))
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

    fn store_onetime(&self, amount: Entity<InputState>, p: Palette, cx: &mut Context<Self>) -> Div {
        let busy = self.store_page.busy;
        let typed = amount.read(cx).value().to_string();
        let donate = div()
            .id("store-onetime")
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(26.))
            .px(px(12.))
            .rounded(px(6.))
            .bg(rgb(YELLOW))
            .text_color(rgb(0x1a1400))
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .when(busy, |d| d.opacity(0.6))
            .when(!busy, |d| d.cursor_pointer().hover(|d| d.opacity(0.9)))
            .child(tr("Donate"))
            .on_click(cx.listener(move |this, _, _, cx| {
                StorePage::checkout(this, Kind::OneTime, Some(typed.clone()), cx)
            }));
        h_flex()
            .flex_shrink_0()
            .max_w_full()
            .h(px(30.))
            .justify_center()
            .gap(px(10.))
            .child(
                Icon::new(IconName::Gift)
                    .size(px(15.))
                    .flex_shrink_0()
                    .text_color(rgb(YELLOW)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("One-time donation")),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(tr("Converts to whole months of a tier")),
            )
            .child(
                div().w(px(96.)).flex_shrink_0().child(
                    Input::new(&amount)
                        .small()
                        .prefix(div().text_color(p.text_muted).child("$")),
                ),
            )
            .child(donate)
    }
}
