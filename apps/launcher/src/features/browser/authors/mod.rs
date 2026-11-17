//! The people behind the addons: publisher profiles, the Creators list, "more
//! from" lists, and the trail of views the browser walks back along.

use std::collections::BTreeSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::*;
use swofty_core::catalog::Addon;
use swofty_core::catalog::publishers::{self, Creator};
use swofty_i18n::{tr, trf, trn};

use super::*;
use crate::shell::Shell;
use crate::ui::format;
use crate::ui::label;
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;

mod byline;
mod creators;
mod profile;
mod related;
mod trail;

const MORE_SHOWN: usize = 4;
const MORE_INSTALLED: usize = 3;
const CREATORS_FOUND: usize = 4;
const CRUMBS_SHOWN: usize = 4;
const CREATOR_W: f32 = 300.;
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// What the browser shows. `Installed` is the Addons page, for the way back to it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum View {
    Grid,
    Creators,
    Addon(String),
    Author(String),
    Installed(String),
}

fn joined(iso: &str) -> Option<String> {
    let year = iso.get(..4)?.parse::<u32>().ok()?;
    let month = iso.get(5..7)?.parse::<usize>().ok()?;
    Some(trf!(
        "Joined {month} {year}",
        month = tr(MONTHS.get(month.checked_sub(1)?)?),
        year = year
    ))
}

fn face_color(id: &str, p: Palette) -> Hsla {
    let hash = id
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    Hsla {
        h: (p.accent.h + (hash % 12) as f32 / 12.).fract(),
        ..p.accent
    }
}

fn author_links(links: &[publishers::Link]) -> Vec<AnyElement> {
    links
        .iter()
        .filter(|l| l.url.starts_with("https://"))
        .enumerate()
        .map(|(i, l)| {
            let url = l.url.clone();
            let text = match l.label.trim() {
                "" => host(&l.url).to_owned(),
                label => label.to_owned(),
            };
            Button::new(SharedString::from(format!("browser-author-link-{i}")))
                .outline()
                .tap_sm()
                .icon(IconName::ExternalLink)
                .label(text)
                .tooltip(url.clone())
                .on_click(move |_, _, cx| cx.open_url(&url))
                .into_any_element()
        })
        .collect()
}

fn author_totals(totals: publishers::Totals, p: Palette) -> Div {
    let figure = |title: &'static str, value: String| {
        v_flex()
            .flex_1()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value),
            )
            .child(label(title, p))
    };
    h_flex()
        .px(px(24.))
        .py(px(16.))
        .gap(px(24.))
        .border_t_1()
        .border_color(p.border_variant)
        .child(figure(tr("ADDONS"), totals.addons.to_string()))
        .child(figure(tr("DOWNLOADS"), format::compact(totals.downloads)))
        .child(figure(
            tr("DOWNLOADS THIS MONTH"),
            format::compact(totals.downloads_30d),
        ))
        .child(figure(tr("STARS"), format::compact(totals.stars)))
}

fn text_link(key: String, text: impl Into<SharedString>, p: Palette) -> Stateful<Div> {
    h_flex()
        .id(SharedString::from(key))
        .gap(px(4.))
        .flex_shrink_0()
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.accent)
        .hover(move |s| s.text_color(p.accent_hover).underline())
        .child(text.into())
}

fn stats_line(addons: u64, downloads: u64, stars: u64, p: Palette) -> Div {
    h_flex()
        .gap(px(10.))
        .text_size(px(12.))
        .text_color(p.text_placeholder)
        .whitespace_nowrap()
        .child(
            h_flex()
                .gap(px(4.))
                .child(Icon::new(IconName::Package).size(px(12.)))
                .child(trn(addons as usize, "{count} addon", "{count} addons")),
        )
        .child(
            h_flex()
                .gap(px(4.))
                .child(Icon::new(IconName::Download).size(px(12.)))
                .child(format::compact(downloads)),
        )
        .child(
            h_flex()
                .gap(px(4.))
                .child(Icon::new(IconName::Star).size(px(12.)))
                .child(format::compact(stars)),
        )
}

impl Browser {
    pub(super) fn all_creators(&self) -> Vec<Creator> {
        if self.creators.is_empty() {
            publishers::from_catalog(&self.addons)
        } else {
            self.creators.clone()
        }
    }

    fn creator(&self, id: &str) -> Option<Creator> {
        self.creators.iter().find(|c| c.id == id).cloned().or_else(|| {
            publishers::from_catalog(&self.addons)
                .into_iter()
                .find(|c| c.id == id)
        })
    }

    fn publisher_name(&self, id: &str) -> String {
        self.profiles
            .get(id)
            .map(|pr| pr.name.clone())
            .or_else(|| self.creator(id).map(|c| c.name))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| tr("Creator").to_owned())
    }

    fn view_label(&self, v: &View) -> String {
        match v {
            View::Grid => match self.tab {
                None => tr("All addons").to_owned(),
                tab => tab_label(tab).to_owned(),
            },
            View::Creators => tr("Creators").to_owned(),
            View::Addon(id) => self.find(id).map_or_else(|| id.clone(), |a| a.name.clone()),
            View::Author(id) => self.publisher_name(id),
            View::Installed(_) => tr("Addons").to_owned(),
        }
    }

    fn author_addons(&self, id: &str) -> Vec<&Addon> {
        let listed: BTreeSet<&str> = self
            .profiles
            .get(id)
            .map(|pr| pr.addons.iter().map(|a| a.id.as_str()).collect())
            .unwrap_or_default();
        self.addons
            .iter()
            .filter(|a| a.published_by(id) || listed.contains(a.id.as_str()))
            .collect()
    }
}

impl Shell {}

#[cfg(test)]
mod tests {
    use super::joined;

    #[test]
    fn names_the_month_someone_joined() {
        assert_eq!(
            joined("2025-12-27T13:13:04Z").as_deref(),
            Some("Joined December 2025")
        );
        assert_eq!(joined("2026-01-01").as_deref(), Some("Joined January 2026"));
        assert!(joined("2026-13-01").is_none());
        assert!(joined("").is_none());
    }
}
