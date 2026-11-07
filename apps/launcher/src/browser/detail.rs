use crate::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::catalog::{self, Addon};
use swofty_core::update::current_version;
use swofty_i18n::trn;

use crate::format;
use crate::pages::{GUTTER, label, stat, tag};
use crate::shell::Shell;
use crate::theme::Palette;

use super::*;

/// How many of `versions`, newest first, the Addons page shows before its older-versions toggle.
pub(super) fn shown_versions(versions: &[&catalog::Version], installed: &semver::Version) -> usize {
    let through = versions
        .iter()
        .position(|v| v.version <= *installed)
        .map_or(versions.len(), |i| {
            i + usize::from(versions[i].version == *installed)
        });
    through.max(2).min(versions.len())
}

impl Shell {
    /// One registry version and its changelog, for the Addon Browser and the Addons page.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn release_row(
        &self,
        key: &str,
        v: &catalog::Version,
        first: bool,
        installed: Option<&semver::Version>,
        newer: bool,
        inset: f32,
        mono: &SharedString,
        p: Palette,
    ) -> AnyElement {
        let launcher = current_version();
        let fits = v.accepts(&launcher);
        let current = installed == Some(&v.version);
        v_flex()
            .px(px(inset))
            .py(px(12.))
            .gap(px(6.))
            .when(!first, |d| d.border_t_1().border_color(p.border_variant))
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("v{}", v.version)),
                    )
                    .when(current, |d| d.child(tag("Installed", p).text_color(p.success)))
                    .when(newer, |d| d.child(tag("New", p).text_color(p.accent)))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(format!("{} · {}", day(&v.published_at), format::size(v.size))),
                    ),
            )
            .when(!fits, |d| {
                d.child(div().text_size(px(12.)).text_color(p.warning).child(format!(
                    "Needs launcher {}",
                    v.launcher.as_ref().map_or_else(String::new, |r| r.to_string())
                )))
            })
            .when_some(
                v.requires
                    .as_ref()
                    .filter(|r| !r.is_empty())
                    .map(|r| self.requires_text(r)),
                |d, text| d.child(div().text_size(px(12.)).text_color(p.text_muted).child(text)),
            )
            .child(
                v_flex()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .when(v.changelog.trim().is_empty(), |d| {
                        d.child(div().text_color(p.text_placeholder).child("No changelog"))
                    })
                    .when(!v.changelog.trim().is_empty(), |d| {
                        d.children(crate::news::render_markdown(
                            &format!("{key}-{}", v.version),
                            &v.changelog,
                            p,
                            mono,
                        ))
                    }),
            )
            .into_any_element()
    }

    /// The registry's changelog of an installed addon, for its panel on the Addons page.
    pub(crate) fn changelog_section(
        &mut self,
        id: &str,
        installed: &semver::Version,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.browser.has(id) {
            return None;
        }
        if !self.browser.details.contains_key(id) && self.browser.asked.insert(id.to_owned()) {
            let id = id.to_owned();
            cx.spawn(async move |this, cx| {
                this.update(cx, |s, cx| s.fetch_details(&id, cx)).ok();
            })
            .detach();
        }
        let listed = self.browser.find(id)?;
        let full = self.browser.details.get(id);
        let mut versions: Vec<&catalog::Version> = full.map(Addon::listed).unwrap_or_default();
        for v in listed.listed() {
            if !versions.iter().any(|o| o.version == v.version) {
                versions.push(v);
            }
        }
        versions.sort_by(|a, b| b.version.cmp(&a.version));
        let loading = full.is_none() && self.browser.loading.contains(id);
        if versions.is_empty() && !loading {
            return None;
        }
        let newer = versions.iter().filter(|v| v.version > *installed).count();
        let shown = shown_versions(&versions, installed);
        let older = versions.len() - shown;
        let open = self.browser.history.contains(id);
        let mono = cx.theme().mono_font_family.clone();
        let key = format!("addon-changelog-{id}");
        let rows: Vec<AnyElement> = versions
            .iter()
            .take(if open { versions.len() } else { shown })
            .enumerate()
            .map(|(i, v)| {
                self.release_row(
                    &key,
                    v,
                    i == 0,
                    Some(installed),
                    v.version > *installed,
                    0.,
                    &mono,
                    p,
                )
            })
            .collect();
        let summary = if loading {
            "Loading…".to_owned()
        } else if newer > 0 {
            format!(
                "{} since yours",
                trn(newer, "{count} new version", "{count} new versions")
            )
        } else {
            trn(versions.len(), "{count} version", "{count} versions")
        };
        let toggle = (older > 0).then(|| {
            let target = id.to_owned();
            h_flex()
                .id("addon-changelog-older")
                .pt(px(10.))
                .gap(px(6.))
                .items_center()
                .border_t_1()
                .border_color(p.border_variant)
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(p.text_placeholder)
                .hover(move |s| s.text_color(p.text_muted))
                .child(crate::pages::chevron(open, p))
                .child(if open {
                    "Hide older versions".to_owned()
                } else {
                    format!(
                        "Show {}",
                        trn(older, "{count} older version", "{count} older versions")
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.browser.history.remove(&target) {
                        this.browser.history.insert(target.clone());
                    }
                    cx.notify();
                }))
        });
        Some(
            v_flex()
                .child(
                    h_flex()
                        .pb(px(2.))
                        .items_center()
                        .child(div().flex_1().child(label("CHANGELOG", p)))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_placeholder)
                                .child(summary),
                        ),
                )
                .children(rows)
                .children(toggle)
                .into_any_element(),
        )
    }

    pub(super) fn browser_detail(&self, id: &str, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(listed) = self.browser.find(id) else {
            return div().into_any_element();
        };
        let full = self.browser.details.get(id);
        let a = full.unwrap_or(listed);
        let launcher = current_version();
        let mono = cx.theme().mono_font_family.clone();
        let installed = self.installed_version(&a.id);
        let offered = a.offered(&launcher).or_else(|| a.newest());
        let back = self.back_bar(p, cx);
        let homepage = a
            .homepage
            .clone()
            .filter(|u| u.starts_with("https://"))
            .map(|url| {
                Button::new("browser-homepage")
                    .ghost()
                    .small()
                    .icon(IconName::ExternalLink)
                    .label("Homepage")
                    .on_click(move |_, _, cx| cx.open_url(&url))
            });
        let source_code = a.source_url().map(str::to_owned).map(|url| {
            Button::new("browser-source-code")
                .ghost()
                .small()
                .icon(IconName::ExternalLink)
                .label("Source code")
                .on_click(move |_, _, cx| cx.open_url(&url))
        });
        let hero = h_flex()
            .gap(px(16.))
            .items_start()
            .flex_shrink_0()
            .child(self.catalog_icon(a, 64., p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(22.))
                            .line_height(px(28.))
                            .font_weight(FontWeight::BOLD)
                            .child(a.name.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .min_w_0()
                            .child(self.byline(a, "browser-detail-by".into(), 13., p, cx))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(13.))
                                    .text_color(p.text_placeholder)
                                    .child(format!("· {}", a.id)),
                            ),
                    )
                    .children(self.tags(a, "browser-detail-native".into(), p)),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .flex_shrink_0()
                    .child(self.star_button(listed, "browser-detail-star".into(), true, p, cx))
                    .children(homepage)
                    .children(source_code)
                    .child(self.offer_button(listed, "browser-detail-offer".into(), cx)),
            );
        let working = self.browser.working.get(&a.id).map(|job| {
            let pr = job.progress();
            let item = job.item();
            let also = job.also();
            v_flex()
                .gap(px(6.))
                .p(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(p.accent.opacity(0.4))
                .bg(p.accent.opacity(0.06))
                .child(div().text_size(px(13.)).child(pr.long(item.as_deref())))
                .when(!also.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(format!("Also installing: {}", also.join(", "))),
                    )
                })
                .child(
                    div()
                        .h(px(4.))
                        .w_full()
                        .rounded(px(2.))
                        .bg(p.border_variant)
                        .child(
                            div()
                                .h_full()
                                .rounded(px(2.))
                                .bg(p.accent)
                                .w(relative(pr.fraction())),
                        ),
                )
        });
        let failure = self.browser.failed.get(&a.id).map(|e| {
            let target = a.id.clone();
            h_flex()
                .gap(px(10.))
                .items_start()
                .p(px(12.))
                .rounded(px(8.))
                .bg(p.error.opacity(0.08))
                .border_1()
                .border_color(p.error.opacity(0.4))
                .child(Icon::new(IconName::CircleAlert).size(px(15.)).text_color(p.error))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(
                            if self.browser.refused.contains(&a.id) {
                                "Missing requirements"
                            } else if installed.is_some() {
                                "The update failed"
                            } else {
                                "The install failed"
                            },
                        ))
                        .child(div().text_size(px(13.)).text_color(p.text_muted).child(e.clone())),
                )
                .child(
                    Button::new("browser-detail-retry")
                        .outline()
                        .small()
                        .icon(IconName::RotateCw)
                        .label("Retry")
                        .on_click(cx.listener(move |this, _, _, cx| this.install_from_catalog(&target, cx))),
                )
        });
        let needs = (!self.browser.working.contains_key(&a.id)
            && !self.browser.failed.contains_key(&a.id)
            && installed.is_none())
        .then(|| self.requirements(listed))
        .flatten()
        .and_then(|plan| match plan {
            Ok(plan) if plan.also().is_empty() => None,
            Ok(plan) => Some((
                false,
                "Also installs".to_owned(),
                plan.also()
                    .iter()
                    .map(|s| match &s.replaces {
                        Some(old) => format!("{} (replaces {old})", s.label()),
                        None => s.label(),
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            )),
            Err(e) => Some((true, "Missing requirements".to_owned(), e.to_string())),
        })
        .map(|(bad, title, text)| {
            let tone = if bad { p.warning } else { p.accent };
            h_flex()
                .gap(px(10.))
                .items_start()
                .p(px(12.))
                .rounded(px(8.))
                .bg(tone.opacity(0.06))
                .border_1()
                .border_color(tone.opacity(0.4))
                .child(
                    Icon::new(if bad {
                        IconName::TriangleAlert
                    } else {
                        IconName::Package
                    })
                    .size(px(15.))
                    .text_color(tone),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(div().text_size(px(13.)).text_color(p.text_muted).child(text)),
                )
        });
        let requirement = offered
            .and_then(|v| v.launcher.as_ref())
            .map_or_else(|| "Any launcher".to_owned(), |r| r.to_string());
        let fits = offered.is_some_and(|v| v.accepts(&launcher));
        let sha = offered
            .map(|v| v.sha256.chars().take(12).collect::<String>())
            .unwrap_or_default();
        let facts = h_flex()
            .flex_wrap()
            .gap_x(px(28.))
            .gap_y(px(14.))
            .p(px(16.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(stat(
                "STARS",
                format::compact(self.browser.stars_of(listed)),
                p.text,
                p,
            ))
            .child(stat(
                "DOWNLOADS",
                format!(
                    "{} · {} in 30 days",
                    format::compact(a.downloads),
                    format::compact(a.downloads_30d)
                ),
                p.text,
                p,
            ))
            .child(stat(
                "SIZE",
                offered.map(|v| format::size(v.size)).unwrap_or_default(),
                p.text,
                p,
            ))
            .child(stat(
                "LAST UPDATED",
                since(&a.updated_at).unwrap_or_else(|| "–".into()),
                p.text,
                p,
            ))
            .child(stat("FIRST PUBLISHED", day(&a.created_at), p.text, p))
            .child(stat(
                "LAUNCHER",
                requirement,
                if fits { p.text } else { p.warning },
                p,
            ))
            .child(
                v_flex().gap(px(3.)).child(label("SHA-256", p)).child(
                    div()
                        .text_size(px(13.))
                        .font_family(mono.clone())
                        .child(format!("{sha}…")),
                ),
            )
            .when_some(installed.clone(), |d, v| {
                d.child(stat("INSTALLED", format!("v{v}"), p.success, p))
            });
        let description = if a.description.trim().is_empty() {
            vec![
                div()
                    .text_color(p.text.opacity(0.84))
                    .child(if a.summary.is_empty() {
                        "No description yet.".to_owned()
                    } else {
                        a.summary.clone()
                    })
                    .into_any_element(),
            ]
        } else {
            crate::news::render_markdown(&format!("browser-{}", a.id), &a.description, p, &mono)
        };
        let about = v_flex()
            .p(px(24.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .text_size(px(14.))
            .line_height(px(22.))
            .child(div().pb(px(12.)).child(label("ABOUT", p)))
            .children(description);
        let people = self.published_by(listed, p, cx);
        let more = self.more_from(listed, p, cx);
        let listed_versions = a.listed();
        let key = format!("browser-{}", a.id);
        let count = listed_versions.len();
        let versions: Vec<AnyElement> = listed_versions
            .into_iter()
            .enumerate()
            .map(|(i, v)| self.release_row(&key, v, i == 0, installed.as_ref(), false, 16., &mono, p))
            .collect();
        let changelog = v_flex()
            .rounded(px(10.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .child(
                h_flex()
                    .px(px(16.))
                    .h(px(46.))
                    .items_center()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Versions"),
                    )
                    .child(div().flex_1())
                    .child(div().text_size(px(12.)).text_color(p.text_placeholder).child(
                        if full.is_none() && self.browser.loading.contains(id) {
                            "Loading…".to_owned()
                        } else {
                            trn(count, "{count} version", "{count} versions")
                        },
                    )),
            )
            .children(versions);
        let body = v_flex()
            .id("browser-detail")
            .px(px(GUTTER))
            .pb(px(24.))
            .gap(px(18.))
            .child(back)
            .child(hero)
            .children(working)
            .children(failure)
            .children(needs)
            .child(facts)
            .child(
                h_flex()
                    .items_start()
                    .gap(px(16.))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(16.))
                            .children(self.trend_chart(a, p))
                            .child(about),
                    )
                    .child(
                        v_flex()
                            .w(px(SIDE_W))
                            .flex_shrink_0()
                            .gap(px(16.))
                            .children(people)
                            .children(more)
                            .child(changelog),
                    ),
            );
        self.scroll_area(body, cx).into_any_element()
    }
}
