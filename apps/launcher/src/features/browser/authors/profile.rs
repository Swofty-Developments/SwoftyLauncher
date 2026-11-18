//! One publisher's page.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::catalog::publishers::Creator;
use swofty_core::catalog::search::Query;
use swofty_i18n::{tr, trf};

use super::{author_links, author_totals, joined, text_link};
use crate::features::browser::*;
use crate::shell::Shell;
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;
use crate::ui::tokens::space::GUTTER;

impl Shell {
    pub(super) fn author_sort_button(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let shell = cx.entity().downgrade();
        let sort = self.browser.author_sort;
        Button::new("browser-author-sort")
            .outline()
            .tap_sm()
            .icon(IconName::ArrowUpDown)
            .label(sort.label())
            .dropdown_caret(true)
            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                for s in [Sort::Stars, Sort::Downloads, Sort::Updated] {
                    let shell = shell.clone();
                    menu = menu.item(PopupMenuItem::new(s.label()).checked(s == sort).on_click(
                        move |_, _, cx| {
                            shell
                                .update(cx, |this, cx| {
                                    this.browser.author_sort = s;
                                    cx.notify();
                                })
                                .ok();
                        },
                    ));
                }
                menu
            })
    }

    pub(super) fn author_head(&self, id: &str, p: Palette, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let profile = b.profiles.get(id);
        let creator = b.creator(id);
        let name = b.publisher_name(id);
        let avatar = profile
            .and_then(|pr| pr.avatar.clone())
            .or_else(|| creator.as_ref().and_then(|c| c.avatar.clone()));
        let bio = profile
            .map(|pr| pr.bio.clone())
            .or_else(|| creator.as_ref().map(|c| c.bio.clone()))
            .unwrap_or_default();
        let since = profile
            .map(|pr| pr.joined.clone())
            .or_else(|| creator.as_ref().map(|c| c.joined.clone()))
            .and_then(|j| joined(&j));
        let totals = profile
            .map(|pr| pr.totals)
            .or_else(|| creator.as_ref().map(Creator::totals))
            .unwrap_or_default();
        let loading = profile.is_none() && b.profile_loading.contains(id);
        let links = author_links(profile.map(|pr| pr.links.as_slice()).unwrap_or_default());
        let paragraphs: Vec<AnyElement> = bio
            .lines()
            .map(|l| div().min_h(px(8.)).child(l.to_owned()).into_any_element())
            .collect();
        let about = v_flex()
            .flex_1()
            .min_w_0()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(24.))
                    .line_height(px(30.))
                    .font_weight(FontWeight::BOLD)
                    .child(name.clone()),
            )
            .when_some(since, |d, since| {
                d.child(
                    h_flex()
                        .gap(px(6.))
                        .text_size(px(13.))
                        .text_color(p.text_placeholder)
                        .child(Icon::new(IconName::CalendarDays).size(px(13.)))
                        .child(since),
                )
            })
            .child(
                v_flex()
                    .max_w(px(720.))
                    .text_size(px(14.))
                    .line_height(px(22.))
                    .text_color(if paragraphs.is_empty() {
                        p.text_placeholder
                    } else {
                        p.text.opacity(0.84)
                    })
                    .when(paragraphs.is_empty() && !loading, |d| d.child(tr("No bio yet.")))
                    .children(paragraphs),
            )
            .when(!links.is_empty(), |d| {
                d.child(h_flex().pt(px(4.)).gap(px(8.)).flex_wrap().children(links))
            })
            .children(self.author_status(id, p, cx));
        v_flex()
            .flex_shrink_0()
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                h_flex()
                    .p(px(24.))
                    .gap(px(20.))
                    .items_start()
                    .child(self.publisher_avatar(id, &name, avatar.as_deref(), 84., p))
                    .child(about),
            )
            .child(author_totals(totals, p))
    }

    pub(super) fn author_status(&self, id: &str, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let b = &self.browser;
        let profile = b.profiles.get(id);
        let loading = profile.is_none() && b.profile_loading.contains(id);
        let failed = profile.is_none().then(|| b.profile_failed.get(id)).flatten();
        let target = id.to_owned();
        if loading {
            return Some(
                h_flex()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(Spinner::new().icon(IconName::LoaderCircle).with_size(px(12.)))
                    .child(tr("Loading the profile…"))
                    .into_any_element(),
            );
        }
        failed.map(|e| {
            let tip = e.clone();
            h_flex()
                .id("browser-author-failed")
                .gap(px(8.))
                .text_size(px(12.))
                .text_color(p.warning)
                .child(Icon::new(IconName::CircleAlert).size(px(13.)))
                .child(tr("Couldn't load the full profile"))
                .child(
                    text_link("browser-author-retry".into(), tr("Retry"), p).on_click(
                        cx.listener(move |this, _, _, cx| Browser::fetch_profile(this, &target, cx)),
                    ),
                )
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .into_any_element()
        })
    }

    pub(in crate::features::browser) fn author_page(
        &self,
        id: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let b = &self.browser;
        let name = b.publisher_name(id);
        let loading = !b.profiles.contains_key(id) && b.profile_loading.contains(id);
        let head = self.author_head(id, p, cx);
        let mut addons = b.author_addons(id);
        let sort = b.author_sort;
        addons.sort_by(|x, y| {
            let first = match sort {
                Sort::Downloads => y.downloads.cmp(&x.downloads),
                Sort::Updated => y.updated_at.cmp(&x.updated_at),
                _ => b.stars_of(y).cmp(&b.stars_of(x)),
            };
            first.then_with(|| x.name.to_lowercase().cmp(&y.name.to_lowercase()))
        });
        let count = addons.len();
        let rows = self.card_rows(&addons, &Query::default(), self.layout.content_width, p, cx);
        let empty = (count == 0).then(|| {
            div()
                .text_size(px(13.))
                .text_color(p.text_placeholder)
                .child(if loading {
                    tr("Loading their addons…")
                } else {
                    tr("No listed addons right now.")
                })
        });
        let body = v_flex()
            .id("browser-author")
            .px(px(GUTTER))
            .pb(px(24.))
            .gap(px(GAP))
            .child(self.back_bar(p, cx))
            .child(head)
            .child(
                h_flex()
                    .pt(px(6.))
                    .gap(px(10.))
                    .items_center()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(trf!("Addons by {name}", name = name)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(count.to_string()),
                    )
                    .child(div().flex_1())
                    .child(self.author_sort_button(cx)),
            )
            .children(rows)
            .children(empty);
        self.scroll_area(body, cx).into_any_element()
    }
}
