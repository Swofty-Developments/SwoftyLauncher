use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::support::{BIO_MAX, FALLBACK_URL, Goal, Kind, Supporter, format_date, tier_name};
use swofty_i18n::{tr, trf};

use crate::pages::GUTTER;
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use crate::time_trial::Load;
use crate::ui::EmptyState;

const LIST_WIDTH: f32 = 300.;
const PINK: u32 = 0xff5fa2;
const HOT_PINK: u32 = 0xff2d95;
const PURPLE: u32 = 0x9b30ff;
const PLUS_NAME: u32 = 0xd45af0;

fn gradient() -> Background {
    linear_gradient(
        90.,
        linear_color_stop(rgb(HOT_PINK), 0.),
        linear_color_stop(rgb(PURPLE), 1.),
    )
}

fn money(cents: u64) -> String {
    match cents % 100 {
        0 => format!("${}", cents / 100),
        c => format!("${}.{c:02}", cents / 100),
    }
}

fn card(p: Palette) -> Div {
    v_flex()
        .gap(px(10.))
        .p(px(16.))
        .rounded(px(10.))
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
}

fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

fn muted(text: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text_size(px(12.))
        .text_color(p.text_muted)
        .child(text.into())
}

fn badge(plus: bool) -> Div {
    div()
        .flex_shrink_0()
        .px(px(8.))
        .py(px(2.))
        .rounded_full()
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(0xffffff))
        .map(|d| match plus {
            true => d.bg(gradient()),
            false => d.bg(rgb(PINK)),
        })
        .child(tier_name(if plus { "supporter_plus" } else { "supporter" }))
}

impl Shell {
    pub(crate) fn support_page(
        &mut self,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refresh = Button::new("support-refresh")
            .outline()
            .icon(IconName::ArrowRightLeft)
            .label(tr("Refresh"))
            .on_click(cx.listener(|this, _, _, cx| this.support_reload(cx)));
        let inputs = self.support_inputs(window, cx);
        if !self.support.bio_synced
            && let Load::Ready(me) = &self.support.me
        {
            let bio = me.bio.clone().unwrap_or_default();
            inputs.0.update(cx, |s, cx| s.set_value(bio, window, cx));
            self.support.bio_synced = true;
        }
        let list = self.support_list(p);
        let main = self.support_main(inputs, p, cx);
        let header = self.header(
            tr("Support"),
            tr("Keep Swofty Launcher free and open source").to_owned(),
            refresh,
            p,
        );
        v_flex()
            .size_full()
            .child(header)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .gap(px(16.))
                    .px(px(GUTTER))
                    .pb(px(16.))
                    .child(list)
                    .child(main),
            )
            .into_any_element()
    }

    fn support_list(&self, p: Palette) -> AnyElement {
        let frame = || {
            v_flex()
                .w(px(LIST_WIDTH))
                .flex_shrink_0()
                .h_full()
                .min_h_0()
                .rounded(px(10.))
                .border_1()
                .border_color(p.border)
        };
        let note = |text: String| {
            frame()
                .p(px(12.))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(text)
                .into_any_element()
        };
        let supporters = match &self.support.supporters {
            Load::Idle | Load::Loading => return note(tr("Loading supporters…").to_owned()),
            Load::Failed(e) => return note(e.clone()),
            Load::Ready(s) if s.is_empty() => return note(tr("No supporters yet. Be the first.").to_owned()),
            Load::Ready(s) => s,
        };
        let rows: Vec<AnyElement> = supporters
            .iter()
            .enumerate()
            .map(|(i, s)| self.supporter_row(i, s, p))
            .collect();
        frame()
            .child(
                div()
                    .px(px(14.))
                    .pt(px(12.))
                    .pb(px(4.))
                    .child(heading(tr("Supporters"))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("support-list")
                    .child(v_flex().children(rows)),
            )
            .into_any_element()
    }

    fn supporter_row(&self, i: usize, s: &Supporter, p: Palette) -> AnyElement {
        let name = match (&s.display.name, s.display.anonymous) {
            (Some(n), false) => n.clone(),
            _ => tr("Anonymous Supporter").to_owned(),
        };
        let face = match (&s.display.name, s.display.anonymous) {
            (Some(n), false) => self.face(n, s.display.avatar_url.as_deref(), 36., p),
            _ => div()
                .size(px(36.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(p.surface)
                .child(Icon::new(IconName::User).size(px(18.)).text_color(p.text_muted)),
        };
        let plus = s.plus();
        let face = face.when(plus, |d| d.border_2().border_color(rgb(PURPLE)));
        let bio = s.bio.clone().filter(|b| !b.trim().is_empty());
        h_flex()
            .id(SharedString::from(format!("supporter-{i}")))
            .px(px(14.))
            .py(px(10.))
            .gap(px(10.))
            .items_start()
            .when(i > 0, |d| d.border_t_1().border_color(p.border_variant))
            .child(face)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(if plus { PLUS_NAME } else { PINK }))
                                    .child(name),
                            )
                            .child(badge(plus)),
                    )
                    .children(bio.map(|b| muted(b, p))),
            )
            .into_any_element()
    }

    fn support_main(
        &mut self,
        inputs: (
            Entity<gpui_kit::component::input::InputState>,
            Entity<gpui_kit::component::input::InputState>,
            Entity<gpui_kit::component::input::InputState>,
            Entity<gpui_kit::component::input::InputState>,
        ),
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (bio, amount, email, payment) = inputs;
        let busy = self.support.busy;
        let goal = self.support_goal(p);
        let tiers = h_flex()
            .gap(px(12.))
            .child(self.tier_card(Kind::Supporter, "$5", tr("Supporter"), false, busy, p, cx))
            .child(self.tier_card(Kind::SupporterPlus, "$20", tr("Supporter+"), true, busy, p, cx));
        let typed = amount.read(cx).value().to_string();
        let onetime = card(p)
            .child(heading(tr("One-time donation")))
            .child(muted(
                tr("One-time donations convert to whole months of a tier: $20 is 1 month of Supporter+, $5 is 1 month of Supporter. For example $100 is 5 months of Supporter+."),
                p,
            ))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(div().w(px(220.)).child(Input::new(&amount).small().prefix(div().text_color(p.text_muted).child("$"))))
                    .child(
                        Button::new("support-onetime")
                            .small()
                            .primary()
                            .label(tr("Donate"))
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.support_checkout(Kind::OneTime, Some(typed.clone()), cx)
                            })),
                    ),
            );
        let (e, pay) = (email.clone(), payment.clone());
        let claim = card(p)
            .child(heading(tr("Already paid? Claim your support")))
            .child(muted(
                tr("Paid on Stripe outside the launcher? Enter the email you paid with and the invoice or payment ID to get your tier and Discord role."),
                p,
            ))
            .child(Input::new(&email).small())
            .child(Input::new(&payment).small())
            .child(
                h_flex().gap(px(8.)).child(
                    Button::new("support-claim")
                        .small()
                        .outline()
                        .label(if self.mp.is_some() { tr("Claim") } else { tr("Sign in to claim") })
                        .disabled(busy || self.mp.is_none())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let (a, b) = (e.read(cx).value().to_string(), pay.read(cx).value().to_string());
                            if a.trim().is_empty() || b.trim().is_empty() {
                                this.support.notice = Some((false, tr("Enter both the email and the payment ID.").to_owned()));
                                cx.notify();
                            } else {
                                this.support_claim(a, b, cx);
                            }
                        })),
                ),
            )
            .children(self.support.claimed.as_ref().map(|c| {
                let when = c.expires_at.as_ref().map(format_date).unwrap_or_default();
                let mut line = trf!(
                    "You're {tier}: {months} months left (expires {date})",
                    tier = tier_name(&c.tier),
                    months = c.months_left,
                    date = when
                );
                if c.role_granted {
                    line.push_str(&format!(", {}", tr("role granted in Discord")));
                }
                div().text_size(px(13.)).text_color(rgb(0x4cc38a)).child(line)
            }));
        let status = self.support_status(bio, p, cx);
        let perks = card(p).child(heading(tr("What you get"))).children(
            [
                tr("Access to #f1-supporter-leaks"),
                tr("The #f1-supporter-general chat"),
                tr("Discord embed links and external emoji permissions"),
                tr("Supporting a free and open-source launcher with no ads"),
            ]
            .into_iter()
            .map(|t| {
                h_flex()
                    .gap(px(8.))
                    .child(Icon::new(IconName::Check).size(px(14.)).text_color(rgb(PINK)))
                    .child(div().text_size(px(13.)).child(t))
            }),
        );
        let notice = self.support.notice.as_ref().map(|(ok, text)| {
            div()
                .text_size(px(13.))
                .text_color(if *ok { rgb(0x4cc38a).into() } else { p.error })
                .child(text.clone())
        });
        let fallback = Button::new("support-fallback")
            .small()
            .ghost()
            .label(tr("Other ways to donate"))
            .on_click(|_, _, cx| cx.open_url(FALLBACK_URL));
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .min_h_0()
            .scroll_y("support-main")
            .child(
                v_flex()
                    .gap(px(14.))
                    .pr(px(4.))
                    .children(status)
                    .child(goal)
                    .child(tiers)
                    .child(onetime)
                    .child(claim)
                    .children(notice)
                    .child(perks)
                    .child(h_flex().child(fallback)),
            )
            .into_any_element()
    }

    fn support_goal(&self, p: Palette) -> Div {
        let body: AnyElement = match &self.support.goal {
            Load::Idle | Load::Loading => muted(tr("Loading the goal…"), p).into_any_element(),
            Load::Failed(e) => muted(e.clone(), p).into_any_element(),
            Load::Ready(g) => goal_body(g, p).into_any_element(),
        };
        card(p).child(heading(tr("Monthly goal"))).child(body)
    }

    fn tier_card(
        &self,
        kind: Kind,
        price: &'static str,
        name: &'static str,
        plus: bool,
        busy: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = if plus { "support-plus" } else { "support-basic" };
        card(p)
            .flex_1()
            .min_w_0()
            .when(plus, |d| d.border_color(rgb(PURPLE)))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(badge(plus))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{price}{}", tr(" / month"))),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name),
            )
            .child(
                Button::new(id)
                    .small()
                    .when_else(plus, |b| b.primary(), |b| b.outline())
                    .label(if self.mp.is_some() { tr("Subscribe") } else { tr("Sign in to subscribe") })
                    .disabled(busy || self.mp.is_none())
                    .on_click(cx.listener(move |this, _, _, cx| this.support_checkout(kind, None, cx))),
            )
            .into_any_element()
    }

    fn support_status(
        &mut self,
        bio: Entity<gpui_kit::component::input::InputState>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Load::Ready(me) = &self.support.me else {
            return None;
        };
        if !me.active {
            return None;
        }
        let tier = me.tier.clone().unwrap_or_default();
        let plus = tier == "supporter_plus";
        let when = me.expires_at.as_ref().map(format_date).unwrap_or_default();
        let months = me.months_left.unwrap_or(0);
        let typed = bio.read(cx).value().to_string();
        let count = typed.chars().count();
        let mode = self.support.mode.clone();
        let busy = self.support.busy;
        let choice = |id: &'static str, key: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .small()
                .when_else(mode == key, |b| b.primary(), |b| b.outline())
                .label(tr(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.support.mode = key.to_owned();
                    cx.notify();
                }))
        };
        Some(
            card(p)
                .when(plus, |d| d.border_color(rgb(PURPLE)))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .items_center()
                        .child(badge(plus))
                        .child(heading(trf!(
                            "{months} months left (expires {date})",
                            months = months,
                            date = when
                        ))),
                )
                .child(muted(tr("Your line next to your name in the supporter list"), p))
                .child(Input::new(&bio).small())
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(if count > BIO_MAX { p.error } else { p.text_muted })
                        .child(format!("{count} / {BIO_MAX}")),
                )
                .child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(muted(tr("Show me as"), p))
                        .child(choice("mode-discord", "discord", "Discord name & avatar", cx))
                        .child(choice("mode-anon", "anonymous", "Anonymous", cx)),
                )
                .child(
                    h_flex().child(
                        Button::new("support-save")
                            .small()
                            .primary()
                            .label(tr("Save"))
                            .disabled(busy || count > BIO_MAX)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.support_save(typed.clone(), cx)),
                            ),
                    ),
                )
                .into_any_element(),
        )
    }
}

fn goal_body(g: &Goal, p: Palette) -> Div {
    v_flex()
        .gap(px(8.))
        .child(
            div()
                .h(px(12.))
                .w_full()
                .rounded_full()
                .bg(p.border_variant)
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(relative(g.fraction()))
                        .rounded_full()
                        .bg(gradient()),
                ),
        )
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .child(format!(
                    "{} {} {} {}",
                    money(g.raised_cents),
                    tr("of"),
                    money(g.goal_cents),
                    tr("/ month")
                )),
        )
        .child(muted(tr("It pays for"), p))
        .child(h_flex().gap(px(6.)).flex_wrap().children(g.costs.iter().map(|c| {
            div()
                .px(px(8.))
                .py(px(3.))
                .rounded_full()
                .border_1()
                .border_color(p.border)
                .text_size(px(12.))
                .child(c.clone())
        })))
}
