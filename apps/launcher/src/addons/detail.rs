use super::DETAIL_W;
use super::model::Type;
use super::needs::{Need, NeedKind};
use super::row::{experimental_labels, fallback_icon};
use crate::library::Source;
use crate::pages::{addon_icon, chevron, fact, label, logo, tag};
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::folders::{self};
use swofty_core::manifest::Category;
use swofty_core::profile::VANILLA;

/// The value of the Source code row: a link to the addon's source, or a note that it names none.
pub(super) fn source_code(url: Option<&str>, p: Palette) -> AnyElement {
    let Some(url) = url else {
        return div()
            .text_color(p.text_muted)
            .child("Not listed")
            .into_any_element();
    };
    let target = url.to_owned();
    div()
        .id("detail-source-code")
        .cursor_pointer()
        .text_color(p.accent)
        .hover(|s| s.underline())
        .child(url.to_owned())
        .on_click(move |_, _, cx| cx.open_url(&target))
        .into_any_element()
}

impl Shell {
    pub(super) fn addon_detail(
        &mut self,
        id: &str,
        inline: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let changes = self.changes_of(id);
        let changelog = self
            .library
            .addons
            .iter()
            .find(|a| a.manifest.id == id && a.source != Source::Builtin)
            .map(|a| a.manifest.version.clone())
            .and_then(|v| self.changelog_section(id, &v, p, cx));
        let Some(m) = self.library.addons.iter().find(|a| a.manifest.id == id) else {
            return div().into_any_element();
        };
        let man = &m.manifest;
        let made_for = man
            .compat
            .game_builds
            .iter()
            .map(|b| {
                b.to_string()
                    .trim_start_matches(['>', '<', '=', '^', '~'])
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join(", ");
        let (compat, compat_color) = match self.install.as_ref().and_then(|i| i.build_id) {
            _ if man.compat.game_builds.is_empty() => ("Works with any game version".to_owned(), p.text),
            Some(b) if man.compat.accepts_build(b) => ("Works with this game version".to_owned(), p.success),
            Some(b) => (format!("Made for game build {made_for} (yours: {b})"), p.warning),
            None => (format!("Made for game build {made_for}"), p.text),
        };
        let on = self.addon_on(m);
        let (status, status_color) = if self.vanilla() {
            (format!("Switch away from {VANILLA} to use addons"), p.warning)
        } else if on {
            ("On".to_owned(), p.success)
        } else {
            ("Off".to_owned(), p.text_muted)
        };
        let (positions, loaded) = self.positions();
        let position = positions.get(id).copied();
        let movable = self.movable(m);
        let can_up = folders::step(&self.library.entries, id, true).is_some();
        let can_down = folders::step(&self.library.entries, id, false).is_some();
        let folder = self.library.place(id).map(|k| self.library.folders.name(&k));
        let source = match (&man.origin, m.source) {
            (Some(o), Source::Importer) => match &o.file {
                Some(f) => format!("{} · from {f}", m.source.label()),
                None => m.source.label().to_owned(),
            },
            _ => m.source.label().to_owned(),
        };
        let close = (!inline).then(|| {
            Button::new("detail-close")
                .ghost()
                .small()
                .icon(IconName::X)
                .tooltip("Close")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.open_addon = None;
                    cx.notify();
                }))
        });
        let head = h_flex()
            .gap(px(12.))
            .items_start()
            .child(match &m.icon {
                _ if man.is_builtin() => logo(40.),
                Some(icon) => addon_icon(icon, 40., p),
                None => fallback_icon(m, 40., p),
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(man.name.clone()),
                    )
                    .child(div().text_size(px(13.)).text_color(p.text_muted).child(
                        match man.author.trim() {
                            "" | "Unknown" => man.version.to_string(),
                            author => format!("{} · by {author}", man.version),
                        },
                    )),
            )
            .children(close);
        let order_line = position.map(|i| {
            let (up, down) = (id.to_owned(), id.to_owned());
            h_flex()
                .gap(px(4.))
                .child(div().flex_1().child(format!("{i} of {loaded}")))
                .when(movable, |d| {
                    d.child(
                        Button::new("detail-up")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ArrowUp)
                            .tooltip("Move up")
                            .disabled(!can_up)
                            .on_click(
                                cx.listener(move |this, _, window, cx| this.move_step(&up, true, window, cx)),
                            ),
                    )
                    .child(
                        Button::new("detail-down")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ArrowDown)
                            .tooltip("Move down")
                            .disabled(!can_down)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.move_step(&down, false, window, cx)
                            })),
                    )
                })
        });
        let facts = v_flex()
            .child(fact("Status", div().text_color(status_color).child(status), p))
            .when_some(order_line, |d, line| d.child(fact("Load order", line, p)))
            .when(!self.vanilla(), |d| {
                d.child(fact("Folder", folder.unwrap_or_else(|| "None".to_owned()), p))
            })
            .when(m.category().is_some() || m.source != Source::Builtin, |d| {
                d.child(fact(
                    "Category",
                    m.category().map_or("Uncategorised", Category::label),
                    p,
                ))
            })
            .when(!man.kind.is_empty(), |d| d.child(fact("Type", Type::list(m), p)))
            .when(!man.experimental_features.is_empty(), |d| {
                d.child(fact(
                    "In game",
                    swofty_core::overlay_guard::effects(man).join(", "),
                    p,
                ))
                .child(fact("Experimental features", experimental_labels(m), p))
            })
            .when_some(man.shader.as_ref(), |d, s| {
                d.child(fact("Look", s.lut.clone(), p))
            })
            .child(fact("Source", source, p))
            .child(fact(
                "Game version",
                div().text_color(compat_color).child(compat),
                p,
            ))
            .when(!man.requires.is_empty(), |d| {
                let needs = self.needs(m);
                let text = man
                    .requires
                    .iter()
                    .map(|r| {
                        let named = format!("{} {}", self.addon_name(&r.id), r.version);
                        match needs.iter().find(|n| n.id == r.id).map(|n| &n.kind) {
                            Some(NeedKind::Missing) => format!("{named} (not installed)"),
                            Some(NeedKind::Off) => format!("{named} (off)"),
                            Some(NeedKind::Version { found, .. }) => format!("{named} ({found} installed)"),
                            Some(NeedKind::Below) => format!("{named} (loads first)"),
                            None => named,
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let color = if needs.iter().any(Need::blocks) {
                    p.warning
                } else {
                    p.text
                };
                d.child(fact("Requires", div().text_color(color).child(text), p))
            })
            .when_some(man.license.clone(), |d, l| d.child(fact("License", l, p)))
            .when_some(man.homepage.clone(), |d, h| d.child(fact("Homepage", h, p)))
            .when(m.source == Source::Installed || man.source_url().is_some(), |d| {
                d.child(fact("Source code", source_code(man.source_url(), p), p))
            });
        let advanced_open = self.addons.advanced;
        let advanced = v_flex()
            .child(
                h_flex()
                    .id("detail-advanced")
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .hover(move |s| s.text_color(p.text_muted))
                    .child(chevron(advanced_open, p))
                    .child("Advanced")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.addons.advanced = !this.addons.advanced;
                        cx.notify();
                    })),
            )
            .when(advanced_open, |d| {
                d.child(
                    v_flex()
                        .pt(px(6.))
                        .gap(px(2.))
                        .text_size(px(12.))
                        .child(div().text_color(p.text_muted).child("Addon ID"))
                        .child(
                            div()
                                .id("detail-id")
                                .text_color(p.text_placeholder)
                                .truncate()
                                .child(man.id.clone())
                                .tooltip({
                                    let id = man.id.clone();
                                    move |window, cx| Tooltip::new(id.clone()).build(window, cx)
                                }),
                        ),
                )
            });
        let parts = changes.parts();
        let what = v_flex()
            .gap(px(8.))
            .child(label("WHAT IT CHANGES", p))
            .child(if parts.is_empty() {
                div()
                    .text_size(px(13.))
                    .text_color(p.text_placeholder)
                    .child(if man.launcher.is_some() {
                        "Adds pages and tools to the launcher"
                    } else {
                        "Nothing listed in its manifest"
                    })
                    .into_any_element()
            } else {
                h_flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .children(parts.into_iter().map(|t| tag(t, p).text_color(p.text)))
                    .into_any_element()
            })
            .when(!changes.unreadable.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.warning)
                        .child(format!("Could not read {}", changes.unreadable.join(", "))),
                )
            })
            .children(m.left_out.iter().map(|why| {
                h_flex()
                    .gap(px(6.))
                    .items_start()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(14.))
                            .mt(px(2.))
                            .text_color(p.warning),
                    )
                    .child(div().flex_1().min_w_0().child(why.clone()))
            }));
        let mut actions: Vec<AnyElement> = Vec::new();
        if m.source == Source::DbEditor {
            let id = id.to_owned();
            actions.push(
                Button::new("detail-edit-db")
                    .outline()
                    .small()
                    .icon(IconName::Database)
                    .label("Edit in DB Editor")
                    .on_click(cx.listener(move |this, _, _, cx| this.db_edit_addon(&id, cx)))
                    .into_any_element(),
            );
        }
        let mut remove: Vec<AnyElement> = Vec::new();
        if m.source != Source::Builtin && !m.locked {
            let profile = &self.library.profile;
            if !profile.is_vanilla() && m.in_profile {
                let (id, name) = (man.id.clone(), man.name.clone());
                remove.push(
                    Button::new("detail-take-out")
                        .ghost()
                        .small()
                        .icon(IconName::Minus)
                        .label(format!("Remove from {}", profile.name))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.remove_from_profile(&id, &name, window, cx)
                        }))
                        .into_any_element(),
                );
            }
            let (id, name) = (man.id.clone(), man.name.clone());
            remove.push(
                Button::new("detail-remove")
                    .ghost()
                    .small()
                    .icon(IconName::Trash)
                    .label("Uninstall")
                    .text_color(p.error)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.remove_addon(&id, &name, window, cx)),
                    )
                    .into_any_element(),
            );
        }
        let settings = if man.options.is_empty() {
            None
        } else {
            Some(self.options_panel(man, p, cx).into_any_element())
        };
        let conflicts = self.conflicts_block(id, p, cx).into_any_element();
        let flag = self.problem_box(id, 0., "detail", p, cx);
        let body = v_flex()
            .gap(px(18.))
            .when(!inline, |d| d.child(head))
            .children(flag)
            .when_some(man.description.clone(), |d, text| {
                d.child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .text_color(p.text_muted)
                        .child(text),
                )
            })
            .when(!actions.is_empty() || !remove.is_empty(), |d| {
                d.child(
                    h_flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .when(!actions.is_empty(), |d| d.children(actions).child(div().flex_1()))
                        .children(remove),
                )
            })
            .child(facts)
            .children(self.registry_section(id, p, cx))
            .children(changelog)
            .child(what)
            .child(conflicts)
            .children(settings)
            .child(advanced);
        let key = SharedString::from(format!("detail-{id}"));
        if inline {
            return body
                .id(key)
                .flex_shrink_0()
                .px(px(20.))
                .pt(px(14.))
                .pb(px(18.))
                .bg(p.surface)
                .border_t_1()
                .border_color(p.border_variant)
                .into_any_element();
        }
        div()
            .w(px(DETAIL_W))
            .flex_shrink_0()
            .h_full()
            .rounded(px(8.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .child(body.size_full().p(px(18.)).scroll_y(key))
            .into_any_element()
    }
}
