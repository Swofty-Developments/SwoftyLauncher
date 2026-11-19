use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Disableable, Sizable, h_flex, v_flex};
use gpui_kit::*;
use swofty_core::online::server::support::{format_date, tier_name};
use swofty_i18n::{tr, trf};

use super::StorePage;
use super::parts::*;
use crate::shell::Shell;
use crate::ui::Overlay;
use crate::ui::scale::px;
use crate::ui::theme::Palette;

impl Shell {
    pub(super) fn store_claim_modal(
        &self,
        email: Entity<InputState>,
        payment: Entity<InputState>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.store_page.busy;
        let weak = cx.entity().downgrade();
        let (e, pay) = (email.clone(), payment.clone());
        let note = self
            .store_page
            .notice
            .as_ref()
            .map(|(ok, text)| notice(*ok, text.clone(), p));
        let claimed = self.store_page.claimed.as_ref().map(|c| {
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
            notice(true, line, p)
        });
        let panel = v_flex()
            .w(px(440.))
            .gap(px(12.))
            .p(px(20.))
            .rounded(px(12.))
            .border_1()
            .border_color(p.border)
            .bg(p.background)
            .child(div().text_size(px(17.)).font_weight(FontWeight::BOLD).child(tr("Claim previous donation")))
            .child(muted(
                tr("Paid on Stripe outside the launcher? Enter the email you paid with and the invoice or payment ID to get your tier and Discord role."),
                p,
            ))
            .child(Input::new(&email).small())
            .child(Input::new(&payment).small())
            .children(note)
            .children(claimed)
            .child(
                h_flex()
                    .gap(px(8.))
                    .justify_end()
                    .child(
                        Button::new("store-claim-close")
                            .small()
                            .ghost()
                            .label(tr("Close"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.store_page.claim_open = false;
                                this.store_page.notice = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("store-claim")
                            .small()
                            .primary()
                            .label(if self.account.session.is_some() { tr("Claim") } else { tr("Sign in to claim") })
                            .disabled(busy || self.account.session.is_none())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let (a, b) = (e.read(cx).value().to_string(), pay.read(cx).value().to_string());
                                if a.trim().is_empty() || b.trim().is_empty() {
                                    this.store_page.notice =
                                        Some((false, tr("Enter both the email and the payment ID.").to_owned()));
                                    cx.notify();
                                } else {
                                    StorePage::claim(this, a, b, cx);
                                }
                            })),
                    ),
            );
        Overlay::new("store-claim-modal")
            .on_dismiss(move |_, cx| {
                weak.update(cx, |this, cx| {
                    this.store_page.claim_open = false;
                    cx.notify();
                })
                .ok();
            })
            .child(panel)
            .into_any_element()
    }
}
