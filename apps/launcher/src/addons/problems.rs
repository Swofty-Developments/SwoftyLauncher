use crate::pages::GUTTER;
use crate::scale::px;
use crate::shell::{Page, Shell};
use crate::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Icon, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::*;
use std::collections::HashSet;
use swofty_core::order;
use swofty_i18n::{tr, trf, trn};

impl Shell {
    pub(super) fn installed_manifests(
        &self,
    ) -> std::collections::BTreeMap<String, swofty_core::manifest::AddonManifest> {
        self.library
            .addons
            .iter()
            .map(|a| (a.manifest.id.clone(), a.manifest.clone()))
            .collect()
    }

    pub(super) fn refresh_problems(&mut self) {
        let built = self.addons.report.as_ref().map_or(0, |r| r.built_at.to_bits());
        let key = (self.library.loaded, built);
        if self.addons.problems_key == Some(key) {
            return;
        }
        self.addons.problems_key = Some(key);
        if self.vanilla() {
            self.addons.problems.clear();
            self.addons.cycle = None;
            return;
        }
        let installed = self.installed_manifests();
        let profile = &self.library.profile;
        self.addons.careers =
            swofty_core::stdlib::careers::read(&swofty_core::stdlib::careers::data_path(&self.store.root));
        let tracks: HashSet<&str> = self
            .library
            .addons
            .iter()
            .filter(|a| a.track)
            .map(|a| a.manifest.id.as_str())
            .collect();
        let store = &self.store;
        let pinned = |id: &str| store.is_builtin(id) || order::is_pinned(id);
        let mut found = order::problems(profile, &installed, &pinned);
        if let Some(r) = &self.addons.report {
            found.extend(order::from_report(profile, r, &pinned));
        }
        found.extend(order::careers(profile, &installed, &self.addons.careers));
        found.extend(order::clashes(profile, &installed, &|id| tracks.contains(id)));
        let mut trial = profile.clone();
        self.addons.cycle = order::fix(&mut trial, &installed, &found, &pinned)
            .err()
            .map(|e| e.to_string());
        self.addons.problems = found;
    }

    pub(super) fn problems_for(&self, id: &str) -> Vec<&order::Problem> {
        self.addons.problems.iter().filter(|p| p.addon == id).collect()
    }

    pub(super) fn can_order(&self) -> bool {
        self.addons.problems.iter().any(order::Problem::is_order)
    }

    pub(super) fn problem_color(p: Palette, severity: order::Severity) -> Hsla {
        match severity {
            order::Severity::Red => p.error,
            order::Severity::Amber => p.warning,
        }
    }

    pub(super) fn order_for_me(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let installed = self.installed_manifests();
        let learned = self.addons.problems.clone();
        let store = self.store.clone();
        let moved = std::rc::Rc::new(std::cell::Cell::new(true));
        let seen = moved.clone();
        self.update_profile(window, cx, move |profile| {
            let pinned = |id: &str| store.is_builtin(id) || order::is_pinned(id);
            seen.set(order::fix(profile, &installed, &learned, &pinned)?);
            Ok(())
        });
        if !moved.get() {
            window.push_notification(
                Notification::info("The order already satisfies every rule it can fix. Nothing moved."),
                cx,
            );
        }
    }

    pub(super) fn move_dependency_above(
        &mut self,
        dep: &str,
        addon: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (dep, addon) = (dep.to_owned(), addon.to_owned());
        self.update_profile(window, cx, move |profile| {
            order::move_above(profile, &dep, &addon)
        });
    }

    pub(super) fn install_missing(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.browser.has(id) {
            true => self.install_from_catalog(id, cx),
            false => self.open_page(Page::Browser, cx),
        }
    }

    pub(super) fn problem_actions(
        &self,
        scope: &str,
        pr: &order::Problem,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let name = |id: &str| self.addon_name(id);
        let key = |what: &str| SharedString::from(format!("fix-{scope}-{what}-{}-{}", pr.addon, pr.other));
        let (addon, other) = (pr.addon.clone(), pr.other.clone());
        let button = |id: SharedString, label: String| Button::new(id).small().outline().label(label);
        let mut out: Vec<AnyElement> = Vec::new();
        match &pr.kind {
            order::Kind::Missing => {
                let id = other.clone();
                out.push(
                    button(key("install"), trf!("Install {name}", name = name(&other)))
                        .on_click(cx.listener(move |this, _, _, cx| this.install_missing(&id, cx)))
                        .into_any_element(),
                );
            }
            order::Kind::Off => {
                let id = other.clone();
                out.push(
                    button(key("on"), trf!("Turn {name} on", name = name(&other)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_addon_enabled(&id, true, window, cx)
                        }))
                        .into_any_element(),
                );
            }
            order::Kind::Below
            | order::Kind::Waits
            | order::Kind::Loses {
                winner_below: true, ..
            } => {
                let (dep, at) = (other.clone(), addon.clone());
                out.push(
                    button(key("above"), trf!("Move {name} above", name = name(&other)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.move_dependency_above(&dep, &at, window, cx)
                        }))
                        .into_any_element(),
                );
            }
            order::Kind::Version { .. } => {
                out.push(
                    button(key("update"), tr("Open Addon Browser").to_owned())
                        .on_click(cx.listener(|this, _, _, cx| this.open_page(Page::Browser, cx)))
                        .into_any_element(),
                );
            }
            order::Kind::Conflict | order::Kind::Clash { .. } => {
                let id = other.clone();
                out.push(
                    button(key("off"), trf!("Turn {name} off", name = name(&other)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_addon_enabled(&id, false, window, cx)
                        }))
                        .into_any_element(),
                );
            }
            order::Kind::Loses { .. } => {}
            order::Kind::Career { why, .. } => {
                let id = addon.clone();
                match &**why {
                    order::Kind::Missing => out.push(
                        button(key("install"), trf!("Install {name}", name = name(&addon)))
                            .on_click(cx.listener(move |this, _, _, cx| this.install_missing(&id, cx)))
                            .into_any_element(),
                    ),
                    order::Kind::Off => out.push(
                        button(key("on"), trf!("Turn {name} on", name = name(&addon)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_addon_enabled(&id, true, window, cx)
                            }))
                            .into_any_element(),
                    ),
                    _ => out.push(
                        button(key("update"), tr("Open Addon Browser").to_owned())
                            .on_click(cx.listener(|this, _, _, cx| this.open_page(Page::Browser, cx)))
                            .into_any_element(),
                    ),
                }
            }
        }
        out
    }

    pub(super) fn problem_box(
        &self,
        id: &str,
        indent: f32,
        scope: &'static str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let found = self.problems_for(id);
        if found.is_empty() {
            return None;
        }
        let worst = match found.iter().any(|f| f.severity() == order::Severity::Red) {
            true => order::Severity::Red,
            false => order::Severity::Amber,
        };
        let color = Self::problem_color(p, worst);
        let name = |id: &str| self.addon_name(id);
        let ordering = found
            .iter()
            .any(|f| matches!(f.kind, order::Kind::Below | order::Kind::Waits));
        let lines: Vec<AnyElement> = found
            .iter()
            .map(|f| {
                let line_color = Self::problem_color(p, f.severity());
                h_flex()
                    .gap(px(8.))
                    .items_start()
                    .flex_wrap()
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(14.))
                            .mt(px(2.))
                            .text_color(line_color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .text_size(px(13.))
                            .text_color(p.text)
                            .child(f.line(&name)),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .flex_wrap()
                            .children(self.problem_actions(scope, f, cx)),
                    )
                    .into_any_element()
            })
            .collect();
        let order_button = (ordering && self.addons.cycle.is_none()).then(|| {
            Button::new(SharedString::from(format!("order-{scope}-{id}")))
                .small()
                .outline()
                .icon(IconName::ArrowUpDown)
                .label(tr("Order for me"))
                .tooltip(tr(
                    "Reorders the whole profile so every addon loads below what it needs",
                ))
                .on_click(cx.listener(|this, _, window, cx| this.order_for_me(window, cx)))
        });
        let cycle = ordering.then(|| self.addons.cycle.clone()).flatten();
        Some(
            v_flex()
                .id(SharedString::from(format!("problems-{scope}-{id}")))
                .ml(px(indent + 6.))
                .mr(px(12.))
                .mb(px(8.))
                .px(px(10.))
                .py(px(8.))
                .gap(px(6.))
                .rounded(px(6.))
                .border_1()
                .border_color(color.opacity(0.7))
                .bg(color.opacity(0.12))
                .children(lines)
                .children(cycle.map(|c| div().text_size(px(12.)).text_color(p.text_muted).child(c)))
                .children(order_button)
                .into_any_element(),
        )
    }

    pub(super) fn problem_banner(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.vanilla() || self.addons.problems.is_empty() {
            return None;
        }
        let all = &self.addons.problems;
        let red = all
            .iter()
            .filter(|f| f.severity() == order::Severity::Red)
            .count();
        let amber = all.len() - red;
        let color = Self::problem_color(
            p,
            if red > 0 {
                order::Severity::Red
            } else {
                order::Severity::Amber
            },
        );
        let text = match (red, amber) {
            (r, 0) => trf!(
                "{amount} to fix before these addons work together",
                amount = trn(r, "{count} problem", "{count} problems")
            ),
            (0, a) => trf!(
                "{amount} worth a look",
                amount = trn(a, "{count} warning", "{count} warnings")
            ),
            (r, a) => trf!(
                "{amount} to fix, and {amount2}",
                amount = trn(r, "{count} problem", "{count} problems"),
                amount2 = trn(a, "{count} warning", "{count} warnings")
            ),
        };
        let first = all
            .iter()
            .find(|f| f.severity() == order::Severity::Red)
            .or_else(|| all.first())
            .map(|f| {
                let name = |id: &str| self.addon_name(id);
                format!("{}: {}", self.addon_name(&f.addon), f.line(&name))
            })
            .unwrap_or_default();
        let order_button = (self.can_order() && self.addons.cycle.is_none()).then(|| {
            Button::new("order-banner")
                .small()
                .outline()
                .icon(IconName::ArrowUpDown)
                .label(tr("Order for me"))
                .on_click(cx.listener(|this, _, window, cx| this.order_for_me(window, cx)))
        });
        Some(
            h_flex()
                .mx(px(GUTTER))
                .mb(px(12.))
                .px(px(12.))
                .py(px(8.))
                .gap(px(10.))
                .flex_shrink_0()
                .items_center()
                .flex_wrap()
                .rounded(px(6.))
                .border_1()
                .border_color(color.opacity(0.7))
                .bg(color.opacity(0.12))
                .child(Icon::new(IconName::TriangleAlert).size(px(16.)).text_color(color))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(200.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.text)
                                .child(text),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_muted)
                                .truncate()
                                .child(first),
                        )
                        .children(
                            self.addons
                                .cycle
                                .clone()
                                .map(|c| div().text_size(px(12.)).text_color(p.text_muted).child(c)),
                        ),
                )
                .children(order_button)
                .into_any_element(),
        )
    }
}
