use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::online::server::support::{Preview, format_date};
use swofty_i18n::tr;

use super::parts::*;
use crate::shell::Shell;
use crate::ui::discord;
use crate::ui::scale::px;
use crate::ui::theme::{Palette, ScrollArea};

/// Only links to the web open; anything else from the server is ignored.
fn web_link(link: Option<&str>) -> Option<String> {
    link.map(str::trim)
        .filter(|l| l.len() > 8 && l.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")))
        .map(str::to_owned)
}

impl Shell {
    /// The Supporter+ previews beside the tiers, scrolling inside their own panel. None when there are none.
    pub(super) fn store_previews(&self, p: Palette) -> Option<AnyElement> {
        let previews = &self.store_page.previews;
        if previews.is_empty() {
            return None;
        }
        let cards: Vec<AnyElement> = previews
            .iter()
            .enumerate()
            .map(|(i, x)| self.preview_card(i, x, p))
            .collect();
        Some(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(10.))
                .child(panel_head(
                    IconName::Sparkles,
                    hue(PLUS_NAME),
                    tr("Early access"),
                    tr("Addons Supporter+ members can test before anyone else"),
                    p,
                ))
                .child(
                    div().relative().flex_1().min_h_0().child(
                        div().absolute().top_0().left_0().size_full().child(
                            div()
                                .size_full()
                                .scroll_y("store-previews")
                                .child(v_flex().gap(px(10.)).pr(px(4.)).children(cards)),
                        ),
                    ),
                )
                .into_any_element(),
        )
    }

    fn preview_card(&self, i: usize, x: &Preview, p: Palette) -> AnyElement {
        let picture = x.image.as_deref().and_then(|u| self.fetched_image(u));
        let art = div()
            .relative()
            .overflow_hidden()
            .flex_shrink_0()
            .w(px(160.))
            .min_h(px(120.))
            .map(|d| match picture {
                Some(image) => d.child(
                    img(image)
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
                None => d
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(12.))
                    .bg(diagonal())
                    .child(
                        Icon::new(IconName::Sparkles)
                            .absolute()
                            .right(px(-14.))
                            .bottom(px(-14.))
                            .size(px(80.))
                            .text_color(hue(WHITE).opacity(0.14)),
                    )
                    .child(
                        div()
                            .text_size(px(20.))
                            .line_height(px(24.))
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .text_color(hue(WHITE))
                            .text_center()
                            .child(x.title.clone()),
                    ),
            })
            .child(div().absolute().top(px(8.)).left(px(8.)).child(badge(
                SharedString::from(format!("store-preview-glint-{i}")),
                true,
            )));
        let posted = x.posted.map(|at| format_date(&serde_json::Value::from(at)));
        let highlights = (!x.highlights.is_empty()).then(|| {
            div()
                .grid()
                .grid_cols(2)
                .gap_x(px(12.))
                .gap_y(px(4.))
                .children(x.highlights.iter().map(|h| {
                    h_flex()
                        .min_w_0()
                        .gap(px(6.))
                        .child(
                            Icon::new(IconName::Check)
                                .size(px(13.))
                                .flex_shrink_0()
                                .text_color(hue(PLUS_NAME)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.))
                                .child(h.clone()),
                        )
                }))
        });
        let link = web_link(x.link.as_deref()).map(|url| {
            h_flex().pt(px(2.)).child(discord::link_button(
                SharedString::from(format!("store-preview-link-{i}")),
                tr("View in Discord"),
                url,
                30.,
                12.,
            ))
        });
        let content = v_flex()
            .flex_1()
            .min_w_0()
            .p(px(12.))
            .gap(px(6.))
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(15.))
                            .font_weight(FontWeight::BOLD)
                            .child(x.title.clone()),
                    )
                    .children(posted.map(|d| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(p.text_placeholder)
                            .child(d)
                    })),
            )
            .when(!x.summary.trim().is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .line_height(px(17.))
                        .text_color(p.text_muted)
                        .line_clamp(2)
                        .child(x.summary.clone()),
                )
            })
            .children(highlights)
            .child(div().flex_1())
            .children(link);
        h_flex()
            .items_stretch()
            .w_full()
            .rounded(px(12.))
            .overflow_hidden()
            .border_1()
            .border_color(hue(PURPLE).opacity(0.4))
            .bg(tinted(p, PURPLE, 0.05))
            .child(art)
            .child(content)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::web_link;

    #[test]
    fn only_web_links_open() {
        assert_eq!(
            web_link(Some(" https://discord.com/channels/1/2/3 ")),
            Some("https://discord.com/channels/1/2/3".to_owned())
        );
        assert_eq!(web_link(Some("HTTPS://x.y")), Some("HTTPS://x.y".to_owned()));
        assert_eq!(web_link(Some("http://discord.com")), None);
        assert_eq!(web_link(Some("file:///c:/x.exe")), None);
        assert_eq!(web_link(Some("https://")), None);
        assert_eq!(web_link(None), None);
    }
}
