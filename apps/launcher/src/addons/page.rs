use super::SPLIT_AT;
use crate::pages::{GUTTER, label};
use crate::scale::px;
use crate::shell::{Page, Shell};
use crate::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Icon, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;
use swofty_core::import::{self, Route};
use swofty_core::profile::VANILLA;
use swofty_i18n::{fill, tr, trf, trn};

impl Shell {
    pub(crate) fn pick_addons(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: true,
            prompt: Some("Install".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && !paths.is_empty()
            {
                this.update_in(cx, |s, window, cx| s.install_picked(paths, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    pub(super) fn install_picked(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let registry = self.library.registry.clone();
        let profile = self.library.profile.clone();
        let (addons, other): (Vec<PathBuf>, Vec<PathBuf>) = paths
            .into_iter()
            .partition(|p| import::route(&registry, &profile, p) == Some(Route::Addon));
        if !other.is_empty() {
            self.drop_paths(other, window, cx);
        }
        if addons.is_empty() {
            return;
        }
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    addons
                        .iter()
                        .flat_map(|path| {
                            let name = path.file_name().map_or_else(
                                || path.display().to_string(),
                                |n| n.to_string_lossy().into_owned(),
                            );
                            let store = &store;
                            store.install_addons(path).into_iter().map(move |r| match r {
                                Ok(m) => Ok((swofty_core::tracks::left_out_lines(store, &m), m)),
                                Err(e) => {
                                    crate::app_log::action("install addon").file(path).failed(&e);
                                    Err(format!("{name}: {e}"))
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update_in(cx, |s, window, cx| {
                let mut last = None;
                for r in results {
                    match r {
                        Ok((left_out, m)) => {
                            let text = trf!("Installed {name} {version}", name = m.name, version = m.version);
                            window.push_notification(Notification::success(text), cx);
                            if !left_out.is_empty() {
                                let note = Notification::warning(left_out.join("\n"))
                                    .title(tr("Track can't be used"))
                                    .autohide(false);
                                window.push_notification(note, cx);
                            }
                            last = Some(m.id);
                        }
                        Err(e) => window.push_notification(Notification::error(e), cx),
                    }
                }
                s.reload(cx);
                if let Some(id) = last {
                    s.show_addon(&id, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Turns off every addon except those that are always on and those whose
    /// turning off warns about saves, which keep their own switch.
    pub(super) fn turn_off_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let registry = self.library.registry.clone();
        let profile = &self.library.profile;
        let (mut off, mut kept) = (Vec::new(), Vec::new());
        for m in self.library.listed().filter(|m| m.enabled && !m.locked) {
            match self.turn_off_warning(&m.manifest.id) {
                Some(_) => kept.push(m.manifest.name.clone()),
                None => off.push(m.manifest.id.clone()),
            }
        }
        if off.is_empty() {
            return;
        }
        let before = profile.addons.clone();
        self.update_profile(window, cx, |p| {
            off.iter().try_for_each(|id| registry.set_enabled(p, id, false))
        });
        let shell = cx.entity().downgrade();
        let text = trn(off.len(), "Turned off {count} addon", "Turned off {count} addons");
        let note = Notification::success(text)
            .action(move |_, _, cx| {
                let (shell, before) = (shell.clone(), before.clone());
                let note = cx.entity().downgrade();
                Button::new("undo-turn-off-all")
                    .outline()
                    .small()
                    .label(tr("Undo"))
                    .on_click(move |_, window, cx| {
                        let before = before.clone();
                        shell
                            .update(cx, |s, cx| {
                                s.update_profile(window, cx, |p| {
                                    let rest: Vec<_> = p
                                        .addons
                                        .iter()
                                        .filter(|a| !before.iter().any(|b| b.id == a.id))
                                        .cloned()
                                        .collect();
                                    p.addons = before.into_iter().chain(rest).collect();
                                    Ok(())
                                })
                            })
                            .ok();
                        note.update(cx, |n, cx| n.dismiss(window, cx)).ok();
                    })
            })
            .autohide(true);
        window.push_notification(note, cx);
        if !kept.is_empty() {
            window.push_notification(
                Notification::info(trf!(
                    "Left on: {kept}. Turning it off warns about your saves, so use its own switch.",
                    kept = kept.join(", ")
                )),
                cx,
            );
        }
    }

    pub(crate) fn addons_page(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        self.sync_addons();
        self.refresh_problems();
        let flagged = self.fix_bar(p, cx);
        let mut query = self.addons.search.read(cx).value().trim().to_lowercase();
        let highlight = self.sync_highlight(&mut query, cx);
        let subtitle = self.addons_subtitle();
        let actions = h_flex()
            .gap(px(8.))
            .when(!self.vanilla(), |d| {
                d.child(self.check_button("check-conflicts-page", false, cx))
                    .child(
                        Button::new("turn-off-all")
                            .outline()
                            .icon(IconName::PowerOff)
                            .label(tr("Turn off all"))
                            .on_click(cx.listener(|this, _, window, cx| this.turn_off_all(window, cx))),
                    )
            })
            .child(self.install_computer_button(cx));
        let pending = self.pending_note(p);
        let saves = self.saves_banner(false, p, cx);
        let tracks = self.tracks_note(p, cx);
        let split = self.content_width >= SPLIT_AT;
        let selected = self.open_addon.clone().filter(|id| {
            self.library
                .listed()
                .any(|a| &a.manifest.id == id && self.passes(a, &query, None))
        });
        if selected.is_none() {
            self.open_addon = None;
        }
        let list = self.addon_list(&query, highlight.as_ref(), split, p, cx);
        let detail = selected
            .filter(|_| split)
            .map(|id| self.addon_detail(&id, false, p, cx));
        v_flex()
            .size_full()
            .child(self.header(Page::Addons.label(), subtitle, actions, p))
            .children(saves)
            .children(pending)
            .children(flagged)
            .children(tracks)
            .children(self.install_cards(p, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .px(px(GUTTER))
                    .pb(px(20.))
                    .gap(px(20.))
                    .child(self.filters(&query, p, cx))
                    .child(list)
                    .children(detail),
            )
            .into_any_element()
    }

    pub(crate) fn is_card_dismissed(&self, id: &str) -> bool {
        self.store.config().dismissed_cards.contains(id)
    }

    pub(crate) fn dismiss_card(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        if config.dismissed_cards.insert(id.to_string()) {
            self.store.save_config(&config).ok();
            cx.notify();
        }
    }

    pub(crate) fn install_computer_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let shell = cx.entity().downgrade();
        Button::new("install-addon-computer")
            .outline()
            .icon(IconName::Upload)
            .label(tr("Install addon from computer"))
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let (file, folder) = (shell.clone(), shell.clone());
                menu.item(
                    PopupMenuItem::new(tr("Choose a file…"))
                        .icon(IconName::File)
                        .on_click(move |_, window, cx| {
                            file.update(cx, |s, cx| s.pick_addons(false, window, cx)).ok();
                        }),
                )
                .item(
                    PopupMenuItem::new(tr("Choose a folder…"))
                        .icon(IconName::FolderOpen)
                        .on_click(move |_, window, cx| {
                            folder.update(cx, |s, cx| s.pick_addons(true, window, cx)).ok();
                        }),
                )
            })
    }

    pub(super) fn install_cards(&self, p: Palette, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let show_browser = !self.is_card_dismissed("install-browse");
        let show_importer = !self.is_card_dismissed("install-mod");
        if !show_browser && !show_importer {
            return None;
        }

        let roomy = self.content_width >= 900. && !self.short;
        let short = self.short;
        let card = |id, icon, color, title, body, dismiss| {
            install_card(short, p, id, (icon, color), title, body, dismiss)
        };

        let mut row = h_flex()
            .flex_wrap()
            .items_stretch()
            .px(px(GUTTER))
            .pb(px(if short { 12. } else { 18. }))
            .gap(px(if short { 10. } else { 14. }))
            .flex_shrink_0();

        if show_browser {
            let dismiss = Button::new("dismiss-browse")
                .ghost()
                .xsmall()
                .icon(IconName::X)
                .tooltip(tr("Dismiss"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.dismiss_card("install-browse", cx);
                }));
            let browser = card(
                "install-browse",
                IconName::PackageSearch,
                p.warning,
                tr("Looking for a Swofty Addon?"),
                tr("Browse addons published by the community."),
                Some(dismiss.into_any_element()),
            )
            .border_color(p.border_variant)
            .child(
                Button::new("go-browser")
                    .outline()
                    .small()
                    .label(if roomy {
                        tr("Open Addon Browser")
                    } else {
                        tr("Addon Browser")
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.open_page(Page::Browser, cx))),
            );
            row = row.child(browser);
        }

        if show_importer {
            let dismiss = Button::new("dismiss-importer")
                .ghost()
                .xsmall()
                .icon(IconName::X)
                .tooltip(tr("Dismiss"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.dismiss_card("install-mod", cx);
                }));
            let importer = card(
                "install-mod",
                IconName::FolderInput,
                p.success,
                tr("Installing a mod?"),
                if short {
                    tr("Mods convert into addons")
                } else {
                    tr("Mods convert into Swofty addons using the Simplified Importer.")
                },
                Some(dismiss.into_any_element()),
            )
            .border_color(p.border_variant)
            .child(
                Button::new("go-importer")
                    .outline()
                    .small()
                    .label(if roomy {
                        tr("Go to Simplified Importer")
                    } else {
                        tr("Simplified Importer")
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.open_page(Page::Importer, cx))),
            );
            row = row.child(importer);
        }

        Some(row)
    }

    pub(super) fn list_heading(&self, p: Palette) -> AnyElement {
        let title = match self.vanilla() {
            true => tr("INSTALLED").to_owned(),
            false => trf!(
                "LOAD ORDER IN {name}",
                name = self.library.profile.name.to_uppercase()
            ),
        };
        div()
            .px(px(12.))
            .pt(px(8.))
            .pb(px(6.))
            .child(label(title, p))
            .into_any_element()
    }
}

fn install_card(
    short: bool,
    p: Palette,
    id: &'static str,
    (icon, color): (IconName, Hsla),
    title: &'static str,
    body: &'static str,
    dismiss: Option<AnyElement>,
) -> Stateful<Div> {
    let text = v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(2.))
        .child(
            div()
                .text_size(px(if short { 13. } else { 14. }))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(title),
        )
        .when(!short, |d| {
            d.child(
                div()
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .text_color(p.text_muted)
                    .child(body),
            )
        });
    v_flex()
        .id(id)
        .flex_1()
        .flex_basis(px(280.))
        .min_w_0()
        .items_start()
        .px(px(if short { 12. } else { 16. }))
        .py(px(if short { 10. } else { 12. }))
        .gap(px(if short { 8. } else { 12. }))
        .rounded(px(8.))
        .border_1()
        .bg(p.surface)
        .child(
            h_flex()
                .flex_1()
                .w_full()
                .items_start()
                .gap(px(10.))
                .child(
                    Icon::new(icon)
                        .size(px(if short { 15. } else { 19. }))
                        .flex_shrink_0()
                        .text_color(color),
                )
                .child(text)
                .children(dismiss),
        )
}

impl Shell {
    fn addons_subtitle(&self) -> String {
        let installed = trn(
            self.library.listed().count(),
            "{count} addon installed",
            "{count} addons installed",
        );
        if self.vanilla() {
            trf!(
                "{installed} · {vanilla} plays the game without them",
                installed = installed,
                vanilla = VANILLA
            )
        } else {
            let on = self.library.listed().filter(|m| self.addon_on(m)).count();
            let line = trf!(
                "{installed} · {on} on in {name}",
                installed = installed,
                on = on,
                name = self.library.profile.name
            );
            match self.attention() {
                0 => line,
                n => trf!(
                    "{line} · {amount}",
                    line = line,
                    amount = trn(
                        n,
                        "{count} addon needs attention",
                        "{count} addons need attention"
                    )
                ),
            }
        }
    }

    fn pending_note(&self, p: Palette) -> Option<Div> {
        (!self.vanilla() && !self.library.rebuild.is_empty()).then(|| {
            let note = trn(
                self.library.rebuild.len(),
                "{count} database setting changed since the last build · the next launch rebuilds {name}",
                "{count} database settings changed since the last build · the next launch rebuilds {name}",
            );
            h_flex()
                .px(px(GUTTER))
                .pb(px(12.))
                .gap(px(8.))
                .flex_shrink_0()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(
                    Icon::new(IconName::CircleAlert)
                        .size(px(14.))
                        .text_color(p.warning),
                )
                .child(fill(&note, &[("name", self.library.profile.name.to_string())]))
        })
    }

    fn tracks_note(&self, p: Palette, cx: &mut Context<Self>) -> Option<Div> {
        let hidden = self
            .library
            .addons
            .iter()
            .filter(|m| m.in_profile && m.track)
            .count()
            - self.library.listed().filter(|m| m.track).count();
        self.library.tracks_page.filter(|_| hidden > 0).map(|i| {
            h_flex()
                .px(px(GUTTER))
                .pb(px(12.))
                .gap(px(8.))
                .flex_shrink_0()
                .items_center()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(Icon::new(IconName::Map).size(px(14.)))
                .child(trn(
                    hidden,
                    "{count} track addon is on the Tracks page",
                    "{count} track addons are on the Tracks page",
                ))
                .child(
                    Button::new("open-tracks")
                        .ghost()
                        .small()
                        .label(tr("Open Tracks"))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_page(Page::Section(i), cx))),
                )
        })
    }
}
