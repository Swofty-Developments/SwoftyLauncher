use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
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

const ROW_GAP: f32 = 16.;
const TIERS_MAX: f32 = 1120.;
const CHIP_ROW: f32 = 20.;
const ACTION_W: f32 = 220.;
const ACTION_H: f32 = 34.;
const FIGURE_H: f32 = 48.;

fn perk_row(icon: IconName, text: &'static str, hot: bool, accent: Hsla, p: Palette) -> Div {
    h_flex()
        .items_start()
        .gap(px(10.))
        .when(hot, |d| {
            d.mx(px(-8.))
                .px(px(8.))
                .py(px(3.))
                .my(px(-3.))
                .rounded(px(6.))
                .bg(accent.opacity(0.16))
        })
        .child(
            Icon::new(icon)
                .size(px(16.))
                .mt(px(2.))
                .flex_shrink_0()
                .text_color(accent),
        )
        .child(
            div()
                .min_w_0()
                .text_size(px(14.))
                .line_height(px(20.))
                .text_color(if hot { accent } else { p.text })
                .when(hot, |d| d.font_weight(FontWeight::BOLD))
                .child(text),
        )
}

/// The top of a store column: its chip row, icon and name, a figure row of fixed height, and one muted line.
fn column_head(
    chips: Div,
    icon: IconName,
    name: impl Into<SharedString>,
    accent: Hsla,
    figure: impl IntoElement,
    tagline: &'static str,
    p: Palette,
) -> Div {
    v_flex()
        .items_center()
        .child(chips.h(px(CHIP_ROW)).justify_center().gap(px(6.)))
        .child(
            h_flex()
                .mt(px(10.))
                .gap(px(6.))
                .child(Icon::new(icon).size(px(15.)).text_color(accent))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(accent)
                        .child(name.into()),
                ),
        )
        .child(
            h_flex()
                .mt(px(8.))
                .h(px(FIGURE_H))
                .w_full()
                .justify_center()
                .items_center()
                .child(figure),
        )
        .child(
            div()
                .mt(px(6.))
                .max_w(px(280.))
                .text_center()
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(p.text_muted)
                .child(tagline),
        )
}

/// A tier's price: the amount large, its period small beside it.
fn price(amount: &'static str, color: Hsla, p: Palette) -> Div {
    h_flex()
        .items_end()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(40.))
                .line_height(px(42.))
                .font_weight(FontWeight::EXTRA_BOLD)
                .text_color(color)
                .child(amount),
        )
        .child(
            div()
                .pb(px(7.))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(tr("/ month")),
        )
}

/// A column's main button: full width up to the action size, pinned under the column's content.
fn action(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .overflow_hidden()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .h(px(ACTION_H))
        .w_full()
        .max_w(px(ACTION_W))
        .rounded(px(8.))
        .text_size(px(14.))
        .font_weight(FontWeight::BOLD)
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
                    .child(self.store_supporters(p)),
            )
            .children(claim)
            .children(listing)
            .into_any_element()
    }

    /// The donation column: a one-time gift of any amount, claiming an earlier one, and the supporter's own status.
    fn store_account(&self, amount: Entity<InputState>, p: Palette, cx: &mut Context<Self>) -> Div {
        let busy = self.store_page.busy;
        let typed = amount.read(cx).value().to_string();
        let donate = action("store-onetime")
            .bg(rgb(YELLOW))
            .text_color(rgb(0x1a1400))
            .when(busy, |d| d.opacity(0.6))
            .when(!busy, |d| d.cursor_pointer().hover(|d| d.opacity(0.9)))
            .child(tr("Donate"))
            .child(
                Icon::new(IconName::ArrowRight)
                    .size(px(15.))
                    .text_color(rgb(0x1a1400)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                StorePage::checkout(this, Kind::OneTime, Some(typed.clone()), cx)
            }));
        let claim = Button::new("store-claim-open")
            .outline()
            .icon(IconName::ReceiptText)
            .label(tr("Claim previous donation"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.store_page.claim_open = true;
                this.store_page.notice = None;
                cx.notify();
            }));
        let active = matches!(&self.store_page.me, Load::Ready(me) if me.active);
        let status = match &self.store_page.me {
            _ if self.account.session.is_none() => Some(
                muted(tr("Sign in with Discord to subscribe."), p)
                    .text_size(px(12.))
                    .text_center(),
            ),
            Load::Ready(me) if me.active => {
                let plus = me.tier.as_deref() == Some("supporter_plus");
                let when = me.expires_at.as_ref().map(format_date).unwrap_or_default();
                Some(
                    v_flex()
                        .items_center()
                        .gap(px(4.))
                        .child(badge("store-me-glint", plus))
                        .child(
                            div()
                                .text_center()
                                .text_size(px(12.))
                                .text_color(p.text_muted)
                                .child(trf!(
                                    "{months} months left (expires {date})",
                                    months = me.months_left.unwrap_or(0),
                                    date = when
                                )),
                        ),
                )
            }
            _ => None,
        };
        let edit = active.then(|| {
            Button::new("store-listing-open")
                .outline()
                .small()
                .icon(IconName::Pencil)
                .label(tr("Edit your listing"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.store_page.listing_open = true;
                    this.store_page.notice = None;
                    cx.notify();
                }))
        });
        let manage = self.store_page.subscription().is_some().then(|| {
            Button::new("store-manage-subscription")
                .outline()
                .small()
                .icon(IconName::CreditCard)
                .label(tr("Manage subscription"))
                .on_click(cx.listener(|this, _, _, cx| StorePage::portal(this, None, cx)))
        });
        let note = self
            .store_page
            .notice
            .as_ref()
            .filter(|_| !self.store_page.claim_open && !self.store_page.listing_open)
            .map(|(ok, text)| notice(*ok, text.clone(), p).text_size(px(12.)).text_center());
        v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .px(px(20.))
            .child(column_head(
                h_flex(),
                IconName::Gift,
                tr("One-time donation"),
                hue(YELLOW),
                div().w(px(ACTION_W)).child(
                    Input::new(&amount)
                        .large()
                        .prefix(div().text_color(p.text_muted).child("$")),
                ),
                tr("Converts to whole months of a tier"),
                p,
            ))
            .child(
                v_flex()
                    .flex_grow_1()
                    .w_full()
                    .max_w(px(ACTION_W))
                    .mt(px(20.))
                    .items_center()
                    .child(claim)
                    .children(status.map(|s| s.mt(px(10.))))
                    .children(edit.map(|e| div().mt(px(8.)).child(e)))
                    .children(manage.map(|m| div().mt(px(8.)).child(m)))
                    .children(note.map(|n| n.mt(px(10.)))),
            )
            .child(donate.mt(px(22.)))
    }

    fn store_tiers(&self, amount: Entity<InputState>, p: Palette, cx: &mut Context<Self>) -> Div {
        let current = match &self.store_page.me {
            Load::Ready(me) if me.active => me.tier.as_deref(),
            _ => None,
        };
        let basic = vec![
            (IconName::MegaphoneOff, tr("No ads in the launcher"), false),
            (IconName::KeyRound, tr("Access to #f1-supporter-leaks"), false),
            (
                IconName::MessageCircle,
                tr("The #f1-supporter-general chat"),
                false,
            ),
            (
                IconName::Link,
                tr("Discord embed links and external emoji permissions"),
                false,
            ),
            (
                IconName::Code,
                tr("Supporting a free and open-source launcher"),
                false,
            ),
        ];
        let plus = vec![
            (IconName::Layers, tr("All of Supporter"), false),
            (IconName::Rocket, tr("Early access to addons in testing"), true),
            (IconName::Beaker, tr("Access to private betatesting"), false),
        ];
        let divider = || div().flex_shrink_0().w(px(1.)).my(px(8.)).bg(p.border_variant);
        v_flex().flex_1().min_w_0().items_center().child(
            h_flex()
                .w_full()
                .max_w(px(TIERS_MAX))
                .items_stretch()
                .child(self.store_account(amount, p, cx))
                .child(divider())
                .child(self.tier_column(Kind::Supporter, basic, current == Some("supporter"), p, cx))
                .child(divider())
                .child(self.tier_column(
                    Kind::SupporterPlus,
                    plus,
                    current == Some("supporter_plus"),
                    p,
                    cx,
                )),
        )
    }

    /// One tier as a centred column: its head and perks over its Subscribe button, which sits level with its neighbours'.
    fn tier_column(
        &self,
        kind: Kind,
        perks: Vec<(IconName, &'static str, bool)>,
        current: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let plus = kind == Kind::SupporterPlus;
        let (amount, tagline) = match plus {
            true => ("$20", tr("Play new addons before anyone else")),
            false => ("$5", tr("Back the launcher and lose the ads")),
        };
        let accent = hue(if plus { PLUS_NAME } else { PINK });
        let chips = h_flex().when(current, |d| {
            d.child(chip(tr("Your tier"), hue(GREEN).opacity(0.16), hue(GREEN)))
        });
        v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .px(px(20.))
            .child(column_head(
                chips,
                if plus { IconName::Crown } else { IconName::Heart },
                tier_name(if plus { "supporter_plus" } else { "supporter" }),
                accent,
                price(amount, if plus { accent } else { p.text }, p),
                tagline,
                p,
            ))
            .child(
                v_flex()
                    .flex_grow_1()
                    .max_w_full()
                    .mt(px(20.))
                    .gap(px(10.))
                    .children(
                        perks
                            .into_iter()
                            .map(|(icon, text, hot)| perk_row(icon, text, hot, accent, p)),
                    ),
            )
            .child(self.subscribe_button(kind, cx).mt(px(22.)))
    }

    /// Subscribe, or for a subscriber Manage on their own tier and Switch on the other, which open Stripe's portal.
    fn subscribe_button(&self, kind: Kind, cx: &mut Context<Self>) -> Stateful<Div> {
        let plus = kind == Kind::SupporterPlus;
        let busy = self.store_page.busy;
        let signed_in = self.account.session.is_some();
        let subscribed = self.store_page.subscription().map(|s| s.tier == kind.key());
        let label = match subscribed {
            Some(true) => tr("Manage"),
            Some(false) => tr("Switch"),
            None => tr("Subscribe"),
        };
        action(if plus {
            "store-subscribe-plus"
        } else {
            "store-subscribe"
        })
        .text_color(hue(WHITE))
        .map(|d| match plus {
            true => d
                .bg(gradient())
                .child(glint("store-plus-glint", Duration::from_millis(2600))),
            false => d.bg(rgb(PINK)),
        })
        .when(busy, |d| d.opacity(0.6))
        .when(!busy, |d| d.cursor_pointer().hover(|d| d.opacity(0.9)))
        .child(label)
        .child(
            Icon::new(IconName::ArrowRight)
                .size(px(15.))
                .text_color(hue(WHITE)),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if let Some(own) = subscribed {
                StorePage::portal(this, (!own).then_some(kind), cx);
            } else if signed_in {
                StorePage::checkout(this, kind, None, cx);
            } else {
                this.store_page.notice = Some((false, tr("Sign in with Discord to subscribe.").to_owned()));
                cx.notify();
            }
        }))
    }
}
