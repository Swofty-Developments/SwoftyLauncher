//! The Creators list, and the creators a search finds.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::catalog::publishers::Creator;
use swofty_core::catalog::search::Query;
use swofty_i18n::{tr, trf, trn};

use super::{CREATOR_W, CREATORS_FOUND, View, joined, stats_line, text_link};
use crate::features::browser::*;
use crate::shell::Shell;
use crate::ui::format;
use crate::ui::label;
use crate::ui::scale::px;
use crate::ui::theme::Palette;
use crate::ui::tokens::space::GUTTER;

impl Shell {
    pub(super) fn creator_chip(
        &self,
        c: &Creator,
        query: &Query,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let marks: Vec<(std::ops::Range<usize>, HighlightStyle)> = query
            .marks(&c.name)
            .into_iter()
            .map(|r| {
                (
                    r,
                    HighlightStyle {
                        color: Some(p.accent),
                        background_color: Some(p.accent.opacity(0.12)),
                        ..HighlightStyle::default()
                    },
                )
            })
            .collect();
        let target = c.id.clone();
        h_flex()
            .id(SharedString::from(format!("browser-found-{}", c.id)))
            .w(px(CREATOR_W - 24.))
            .flex_shrink_0()
            .p(px(12.))
            .gap(px(12.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .cursor_pointer()
            .hover(move |s| s.border_color(p.accent).bg(p.hover))
            .child(self.publisher_avatar(&c.id, &c.name, c.avatar.as_deref(), 40., p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(StyledText::new(c.name.clone()).with_highlights(marks)),
                    )
                    .child(stats_line(c.addons, c.downloads, c.stars, p)),
            )
            .child(
                Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .text_color(p.text_placeholder),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.browse(View::Author(target.clone()), cx)))
            .into_any_element()
    }

    /// The creators whose names match the search, to show above the addons.
    pub(in crate::features::browser) fn creators_found(
        &self,
        query: &Query,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if query.is_empty() {
            return None;
        }
        let mut found: Vec<(Creator, u32)> = self
            .browser
            .all_creators()
            .into_iter()
            .filter_map(|c| query.score_name(&c.name).map(|s| (c, s)))
            .collect();
        if found.is_empty() {
            return None;
        }
        found.sort_by(|(a, sa), (b, sb)| sb.cmp(sa).then_with(|| b.downloads.cmp(&a.downloads)));
        let total = found.len();
        let chips: Vec<AnyElement> = found
            .iter()
            .take(CREATORS_FOUND)
            .map(|(c, _)| self.creator_chip(c, query, p, cx))
            .collect();
        let more = (total > CREATORS_FOUND).then(|| {
            text_link(
                "browser-found-all".into(),
                trf!("All {count} creators", count = total),
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| this.browse_creators(cx)))
        });
        Some(
            v_flex()
                .flex_shrink_0()
                .gap(px(10.))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(label(tr("CREATORS"), p))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(p.text_placeholder)
                                .child(total.to_string()),
                        )
                        .child(div().flex_1())
                        .children(more),
                )
                .child(h_flex().flex_wrap().gap(px(GAP)).children(chips))
                .child(
                    div()
                        .pt(px(10.))
                        .border_t_1()
                        .border_color(p.border_variant)
                        .child(label(tr("ADDONS"), p)),
                ),
        )
    }

    pub(super) fn creator_card(
        &self,
        c: &Creator,
        rank: usize,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = c.id.clone();
        let bio = c
            .bio
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_owned();
        v_flex()
            .id(SharedString::from(format!("browser-creator-{}", c.id)))
            .w(px(width))
            .flex_none()
            .p(px(16.))
            .gap(px(12.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .cursor_pointer()
            .hover(move |s| s.border_color(p.accent).bg(p.hover))
            .on_click(cx.listener(move |this, _, _, cx| this.browse(View::Author(target.clone()), cx)))
            .child(
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(self.publisher_avatar(&c.id, &c.name, c.avatar.as_deref(), 48., p))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(c.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.text_placeholder)
                                    .truncate()
                                    .child(joined(&c.joined).unwrap_or_else(|| tr("Publisher").to_owned())),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if rank <= 3 { p.accent } else { p.text_placeholder })
                            .child(format!("#{rank}")),
                    ),
            )
            .child(
                div()
                    .h(px(38.))
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(if bio.is_empty() {
                        p.text_placeholder
                    } else {
                        p.text_muted
                    })
                    .line_clamp(2)
                    .text_ellipsis()
                    .child(if bio.is_empty() {
                        tr("No bio yet.").to_owned()
                    } else {
                        bio
                    }),
            )
            .child(
                h_flex()
                    .gap(px(10.))
                    .child(stats_line(c.addons, c.downloads, c.stars, p))
                    .when(c.downloads_30d > 0, |d| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_placeholder)
                                .whitespace_nowrap()
                                .child(trf!(
                                    "{downloads} this month",
                                    downloads = format::compact(c.downloads_30d)
                                )),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(in crate::features::browser) fn creators_page(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let creators = self.browser.all_creators();
        let width = self.layout.content_width;
        let columns = ((width + GAP) / (CREATOR_W + GAP)).floor().max(1.) as usize;
        let card_w = ((width - GAP * (columns - 1) as f32) / columns as f32).floor() - 1.;
        let rows: Vec<AnyElement> = creators
            .iter()
            .enumerate()
            .collect::<Vec<_>>()
            .chunks(columns)
            .map(|chunk| {
                h_flex()
                    .flex_shrink_0()
                    .items_stretch()
                    .gap(px(GAP))
                    .children(
                        chunk
                            .iter()
                            .map(|(i, c)| self.creator_card(c, i + 1, card_w, p, cx)),
                    )
                    .into_any_element()
            })
            .collect();
        let empty = creators.is_empty().then(|| {
            div()
                .text_size(px(13.))
                .text_color(p.text_placeholder)
                .child(tr("No creators to show yet."))
        });
        let body = v_flex()
            .id("browser-creators-page")
            .px(px(GUTTER))
            .pt(px(18.))
            .pb(px(24.))
            .gap(px(GAP))
            .child(
                v_flex()
                    .gap(px(2.))
                    .pb(px(4.))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .child(tr("Creators")),
                    )
                    .child(div().text_size(px(13.)).text_color(p.text_muted).child(trn(
                        creators.len(),
                        "{count} creator publishing to the registry, most downloaded first",
                        "{count} creators publishing to the registry, most downloaded first",
                    ))),
            )
            .children(rows)
            .children(empty);
        self.scroll_area(body, cx).into_any_element()
    }
}
