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
use crate::ui::theme::Palette;

/// Only links to the web open; anything else from the server is ignored.
fn web_link(link: Option<&str>) -> Option<String> {
    link.map(str::trim)
        .filter(|l| l.len() > 8 && l.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")))
        .map(str::to_owned)
}

impl Shell {
    pub(super) fn store_previews(&self, width: f32, p: Palette) -> Option<AnyElement> {
        let previews = &self.store_page.previews;
        if previews.is_empty() {
            return None;
        }
        let wide = width >= SIDE_BY_SIDE;
        Some(
            v_flex()
                .gap(px(18.))
                .child(section_head(
                    IconName::Sparkles,
                    tr("Supporter+ previews"),
                    tr("Addons Supporter+ members can test before anyone else"),
                    None,
                    p,
                ))
                .children(
                    previews
                        .iter()
                        .enumerate()
                        .map(|(i, x)| self.preview_card(i, x, wide, p)),
                )
                .into_any_element(),
        )
    }

    fn preview_card(&self, i: usize, x: &Preview, wide: bool, p: Palette) -> AnyElement {
        let picture = x.image.as_deref().and_then(|u| self.fetched_image(u));
        let art = div()
            .relative()
            .overflow_hidden()
            .flex_shrink_0()
            .map(|d| match wide {
                true => d.w(px(340.)).min_h(px(240.)),
                false => d.w_full().h(px(190.)),
            })
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
                    .p(px(28.))
                    .bg(diagonal())
                    .child(
                        Icon::new(IconName::Sparkles)
                            .absolute()
                            .right(px(-28.))
                            .bottom(px(-28.))
                            .size(px(160.))
                            .text_color(hue(WHITE).opacity(0.14)),
                    )
                    .child(
                        div()
                            .text_size(px(36.))
                            .line_height(px(40.))
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .text_color(hue(WHITE))
                            .text_center()
                            .child(x.title.clone()),
                    ),
            })
            .child(div().absolute().top(px(14.)).left(px(14.)).child(badge(
                SharedString::from(format!("store-preview-glint-{i}")),
                true,
            )));
        let posted = x.posted.map(|at| format_date(&serde_json::Value::from(at)));
        let highlights = (!x.highlights.is_empty()).then(|| {
            div()
                .grid()
                .grid_cols(if wide { 2 } else { 1 })
                .gap_x(px(20.))
                .gap_y(px(10.))
                .pt(px(4.))
                .children(x.highlights.iter().map(|h| {
                    h_flex()
                        .min_w_0()
                        .gap(px(10.))
                        .child(
                            div()
                                .size(px(22.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(hue(PLUS_NAME).opacity(0.18))
                                .child(
                                    Icon::new(IconName::Check)
                                        .size(px(13.))
                                        .text_color(hue(PLUS_NAME)),
                                ),
                        )
                        .child(div().flex_1().min_w_0().text_size(px(14.)).child(h.clone()))
                }))
        });
        let link = web_link(x.link.as_deref()).map(|url| {
            h_flex().pt(px(8.)).child(discord::link_button(
                SharedString::from(format!("store-preview-link-{i}")),
                tr("View in Discord"),
                url,
                40.,
                16.,
            ))
        });
        let content = v_flex()
            .flex_1()
            .min_w_0()
            .p(px(26.))
            .gap(px(12.))
            .child(
                h_flex()
                    .gap(px(10.))
                    .child(chip(
                        tr("Early access"),
                        hue(PLUS_NAME).opacity(0.16),
                        hue(PLUS_NAME),
                    ))
                    .children(
                        posted.map(|d| div().text_size(px(12.)).text_color(p.text_placeholder).child(d)),
                    ),
            )
            .child(
                div()
                    .text_size(px(24.))
                    .line_height(px(30.))
                    .font_weight(FontWeight::BOLD)
                    .child(x.title.clone()),
            )
            .when(!x.summary.trim().is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(15.))
                        .line_height(px(21.))
                        .text_color(p.text_muted)
                        .child(x.summary.clone()),
                )
            })
            .children(highlights)
            .child(div().flex_1())
            .children(link);
        let card = match wide {
            true => h_flex().items_stretch(),
            false => v_flex(),
        };
        card.w_full()
            .rounded(px(16.))
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
