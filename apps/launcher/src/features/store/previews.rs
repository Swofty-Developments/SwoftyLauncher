use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::online::server::support::{Preview, format_date, tier_name};
use swofty_i18n::tr;

use super::parts::*;
use crate::features::time_trial::Load;
use crate::shell::Shell;
use crate::ui::discord;
use crate::ui::scale::px;
use crate::ui::theme::{Palette, ScrollArea};

const RAIL: f32 = 280.;
const ICON: f32 = 40.;

/// Only links to the web open; anything else from the server is ignored.
fn web_link(link: Option<&str>) -> Option<String> {
    link.map(str::trim)
        .filter(|l| l.len() > 8 && l.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")))
        .map(str::to_owned)
}

impl Shell {
    /// The early access rail beside the tiers, shown to everyone and scrolling inside its own height.
    /// Only a Supporter+ can follow a preview's link. None when there are no previews.
    pub(super) fn store_previews(&self, p: Palette) -> Option<AnyElement> {
        let previews = &self.store_page.previews;
        if previews.is_empty() {
            return None;
        }
        let plus = matches!(&self.store_page.me, Load::Ready(me) if me.active && me.tier.as_deref() == Some("supporter_plus"));
        let rows: Vec<AnyElement> = previews
            .iter()
            .enumerate()
            .map(|(i, x)| self.preview_row(i, x, plus, p))
            .collect();
        let head = h_flex()
            .flex_shrink_0()
            .h(px(26.))
            .gap(px(8.))
            .child(
                Icon::new(IconName::Sparkles)
                    .size(px(14.))
                    .text_color(hue(PLUS_NAME)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("Early access")),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(hue(PLUS_NAME))
                    .child(tier_name("supporter_plus")),
            );
        Some(
            v_flex()
                .w(px(RAIL))
                .flex_shrink_0()
                .pl(px(16.))
                .gap(px(12.))
                .border_l_1()
                .border_color(p.border_variant)
                .child(head)
                .child(
                    div().relative().flex_1().min_h_0().child(
                        div().absolute().top_0().left_0().size_full().child(
                            div()
                                .size_full()
                                .scroll_y("store-previews")
                                .child(v_flex().gap(px(16.)).pr(px(6.)).children(rows)),
                        ),
                    ),
                )
                .into_any_element(),
        )
    }

    /// The preview's addon icon: its own, else the catalog's for its first addon, else a lettered tile.
    fn preview_icon(&self, x: &Preview, p: Palette) -> Div {
        if let Some(image) = x.icon.as_deref().and_then(|u| self.fetched_image(u)) {
            return div()
                .size(px(ICON))
                .flex_shrink_0()
                .rounded(px(ICON * 0.22))
                .overflow_hidden()
                .child(img(image).size_full().object_fit(ObjectFit::Cover));
        }
        if let Some(icon) = x.addons.first().and_then(|id| self.catalog_icon_of(id, ICON, p)) {
            return icon;
        }
        let letter: String = x
            .title
            .trim()
            .chars()
            .take(1)
            .flat_map(char::to_uppercase)
            .collect();
        div()
            .size(px(ICON))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(ICON * 0.22))
            .bg(diagonal())
            .text_size(px(17.))
            .font_weight(FontWeight::EXTRA_BOLD)
            .text_color(hue(WHITE))
            .child(letter)
    }

    fn preview_row(&self, i: usize, x: &Preview, plus: bool, p: Palette) -> AnyElement {
        let posted = x.posted.map(|at| format_date(&serde_json::Value::from(at)));
        let highlights: Vec<&str> = x
            .highlights
            .iter()
            .map(|h| h.trim())
            .filter(|h| !h.is_empty())
            .take(3)
            .collect();
        let line = match highlights.is_empty() {
            false => Some(highlights.join(" · ")),
            true => Some(x.summary.trim().to_owned()).filter(|s| !s.is_empty()),
        };
        let id = SharedString::from(format!("store-preview-link-{i}"));
        let link = web_link(x.link.as_deref()).map(|url| {
            let button = match plus {
                true => discord::link_button(id, tr("View in Discord"), url, 22., 8.),
                false => discord::locked_button(id, tr("View in Discord"), 22., 8.),
            };
            v_flex()
                .items_start()
                .pt(px(4.))
                .gap(px(4.))
                .child(button.gap(px(6.)).rounded(px(6.)).text_size(px(11.)))
                .when(!plus, |d| {
                    d.child(
                        h_flex()
                            .gap(px(4.))
                            .child(Icon::new(IconName::Lock).size(px(11.)).text_color(hue(PLUS_NAME)))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(hue(PLUS_NAME))
                                    .child(tr("You must be Supporter+ to install")),
                            ),
                    )
                })
        });
        h_flex()
            .items_start()
            .gap(px(10.))
            .child(self.preview_icon(x, p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.))
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
                    .children(line.map(|l| {
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(l)
                    }))
                    .children(link),
            )
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
