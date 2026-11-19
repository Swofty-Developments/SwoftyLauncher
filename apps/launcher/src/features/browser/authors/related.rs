//! The publisher sections on an addon's page: who made it, and more from them.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::catalog::Addon;
use swofty_core::catalog::publishers::{self};
use swofty_i18n::{tr, trf, trn};

use super::{MORE_INSTALLED, MORE_SHOWN, View, stats_line, text_link};
use crate::features::browser::*;
use crate::shell::Shell;
use crate::ui::format;
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;
use crate::ui::{label, tag};

impl Shell {
    pub(in crate::features::browser) fn published_by(
        &self,
        a: &Addon,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let owner = a.owner()?;
        let creator = self.browser.creator(&owner.id);
        let bio = creator
            .as_ref()
            .and_then(|c| c.bio.lines().find(|l| !l.trim().is_empty()))
            .map(str::to_owned);
        let target = owner.id.clone();
        let others: Vec<AnyElement> = a
            .publishers
            .iter()
            .filter(|pb| pb.id != owner.id)
            .map(|pb| {
                h_flex()
                    .gap(px(8.))
                    .child(self.publisher_avatar(&pb.id, &pb.name, pb.avatar.as_deref(), 20., p))
                    .child(self.author_link(
                        format!("browser-copublisher-{}", pb.id),
                        &pb.id,
                        &pb.name,
                        13.,
                        p,
                        cx,
                    ))
                    .into_any_element()
            })
            .collect();
        Some(
            v_flex()
                .p(px(16.))
                .gap(px(12.))
                .rounded(px(10.))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .child(label(tr("PUBLISHED BY"), p))
                .child(
                    h_flex()
                        .gap(px(12.))
                        .items_center()
                        .child(self.publisher_avatar(&owner.id, &owner.name, owner.avatar.as_deref(), 40., p))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(3.))
                                .child(self.author_link(
                                    "browser-publisher".into(),
                                    &owner.id,
                                    &owner.name,
                                    14.,
                                    p,
                                    cx,
                                ))
                                .when_some(creator.as_ref(), |d, c| {
                                    d.child(stats_line(c.addons, c.downloads, c.stars, p))
                                }),
                        ),
                )
                .when_some(bio, |d, bio| {
                    d.child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .text_color(p.text_muted)
                            .line_clamp(3)
                            .text_ellipsis()
                            .child(bio),
                    )
                })
                .when(!others.is_empty(), |d| {
                    d.child(v_flex().gap(px(6.)).child(label(tr("WITH"), p)).children(others))
                })
                .child(
                    text_link("browser-publisher-profile".into(), tr("View profile"), p)
                        .child(Icon::new(IconName::ArrowRight).size(px(12.)))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.browse(View::Author(target.clone()), cx)),
                        ),
                ),
        )
    }

    pub(super) fn compact_card(&self, a: &Addon, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let target = a.id.clone();
        h_flex()
            .id(SharedString::from(format!("browser-more-{}", a.id)))
            .px(px(16.))
            .py(px(10.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .child(self.catalog_icon(a, 36., p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(1.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(a.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .truncate()
                            .child(a.summary.clone()),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .items_end()
                    .gap(px(2.))
                    .text_size(px(11.))
                    .text_color(p.text_placeholder)
                    .child(
                        h_flex()
                            .gap(px(3.))
                            .when(self.browser.stars.has(&a.id), |d| d.text_color(p.warning))
                            .child(Icon::new(IconName::Star).size(px(11.)))
                            .child(format::compact(self.browser.stars_of(a))),
                    )
                    .child(
                        h_flex()
                            .gap(px(3.))
                            .child(Icon::new(IconName::Download).size(px(11.)))
                            .child(format::compact(a.downloads)),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_details(&target, cx)))
            .into_any_element()
    }

    pub(in crate::features::browser) fn more_from(
        &self,
        a: &Addon,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let owner = a.owner()?;
        let others = publishers::more_from(&self.browser.addons, a, MORE_SHOWN);
        if others.is_empty() {
            return None;
        }
        let target = owner.id.clone();
        Some(
            v_flex()
                .rounded(px(10.))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .overflow_hidden()
                .child(
                    h_flex()
                        .px(px(16.))
                        .h(px(46.))
                        .gap(px(10.))
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(trf!("More from {name}", name = owner.name)),
                        )
                        .child(
                            text_link("browser-more-all".into(), tr("See all"), p)
                                .child(Icon::new(IconName::ArrowRight).size(px(12.)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.browse(View::Author(target.clone()), cx)
                                })),
                        ),
                )
                .children(others.into_iter().map(|o| self.compact_card(o, p, cx))),
        )
    }

    pub(super) fn small_offer(&self, a: &Addon, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let key = SharedString::from(format!("addon-more-offer-{}", a.id));
        let target = a.id.clone();
        match self.offer(a) {
            Offer::Install => Button::new(key)
                .primary()
                .tap_xs()
                .icon(IconName::Download)
                .label(tr("Install"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    Browser::install(this, &target, cx);
                }))
                .into_any_element(),
            Offer::Failed => Button::new(key)
                .danger()
                .tap_xs()
                .icon(IconName::RotateCw)
                .label(tr("Retry"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.retry_download(&target, cx);
                }))
                .into_any_element(),
            Offer::Queued => Button::new(key)
                .outline()
                .tap_xs()
                .label(tr("Queued"))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_page(crate::shell::Page::Downloads, cx);
                }))
                .into_any_element(),
            Offer::Working(progress) => Button::new(key)
                .primary()
                .tap_xs()
                .loading(true)
                .label(progress.short())
                .disabled(true)
                .into_any_element(),
            Offer::Installed(_) | Offer::Update(..) | Offer::UpdateHere(..) => {
                tag(tr("Installed"), p).text_color(p.success).into_any_element()
            }
            Offer::Separate(_) => tag(tr("Your import has this id"), p)
                .text_color(p.warning)
                .into_any_element(),
            Offer::TooOld => tag(tr("Needs a newer launcher"), p).into_any_element(),
            Offer::Blocked => tag(tr("Missing requirements"), p)
                .text_color(p.warning)
                .into_any_element(),
        }
    }

    pub(super) fn registry_owner(
        &self,
        a: &Addon,
        from: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        a.owner().map(|o| {
            let (from, target) = (from.to_owned(), o.id.clone());
            h_flex()
                .flex_1()
                .min_w_0()
                .gap(px(8.))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(self.publisher_avatar(&o.id, &o.name, o.avatar.as_deref(), 24., p))
                .child(div().flex_shrink_0().child(tr("by")))
                .child(
                    div()
                        .id("addon-registry-author")
                        .min_w_0()
                        .truncate()
                        .cursor_pointer()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.text)
                        .hover(move |s| s.text_color(p.accent).underline())
                        .child(o.name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.browse_from_addons(&from, View::Author(target.clone()), cx)
                        })),
                )
        })
    }

    pub(super) fn registry_more(
        &self,
        a: &Addon,
        from: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let others = publishers::more_from(&self.browser.addons, a, MORE_INSTALLED);
        let rows: Vec<AnyElement> = others
            .iter()
            .map(|o| {
                let (from, target) = (from.to_owned(), o.id.clone());
                h_flex()
                    .id(SharedString::from(format!("addon-more-{}", o.id)))
                    .px(px(8.))
                    .py(px(6.))
                    .mx(px(-8.))
                    .gap(px(10.))
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover))
                    .child(self.catalog_icon(o, 28., p))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(o.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.text_placeholder)
                                    .truncate()
                                    .child(o.summary.clone()),
                            ),
                    )
                    .child(self.small_offer(o, p, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.browse_from_addons(&from, View::Addon(target.clone()), cx)
                    }))
                    .into_any_element()
            })
            .collect();
        (!rows.is_empty()).then(|| {
            let name = a
                .owner()
                .map_or_else(|| tr("this author").to_owned(), |o| o.name.clone());
            v_flex()
                .gap(px(2.))
                .child(
                    div()
                        .pb(px(4.))
                        .child(label(trf!("MORE FROM {name}", name = name.to_uppercase()), p)),
                )
                .children(rows)
        })
    }

    /// The registry's side of an installed addon, for its panel on the Addons page.
    pub(crate) fn registry_section(
        &self,
        id: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let a = self.browser.find(id)?;
        let from = id.to_owned();
        let owner = self.registry_owner(a, &from, p, cx);
        let more = self.registry_more(a, &from, p, cx);
        let view = {
            let from = from.clone();
            text_link("addon-registry-view".into(), tr("View in Addon Browser"), p)
                .child(Icon::new(IconName::ArrowRight).size(px(12.)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.browse_from_addons(&from, View::Addon(from.clone()), cx)
                }))
        };
        Some(
            v_flex()
                .gap(px(10.))
                .child(label(tr("FROM THE ADDON BROWSER"), p))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .items_center()
                        .children(owner)
                        .when(a.owner().is_none(), |d| d.child(div().flex_1()))
                        .child(self.star_button(a, "addon-registry-star".into(), false, p, cx)),
                )
                .child(
                    h_flex()
                        .gap(px(10.))
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child(
                            h_flex()
                                .gap(px(4.))
                                .child(Icon::new(IconName::Download).size(px(12.)))
                                .child(trn(a.downloads as usize, "{count} download", "{count} downloads")),
                        )
                        .when(a.downloads_30d > 0, |d| {
                            d.child(trf!(
                                "{count} this month",
                                count = format::compact(a.downloads_30d)
                            ))
                        }),
                )
                .children(more)
                .child(view)
                .into_any_element(),
        )
    }
}
