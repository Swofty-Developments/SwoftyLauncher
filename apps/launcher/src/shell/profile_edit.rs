use crate::scale::px;
use crate::theme::{SCROLLBAR_GAP, ScrollArea, palette};
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{WindowExt, h_flex, v_flex};
use gpui_kit::*;
use std::collections::BTreeSet;
use std::rc::Rc;
use swofty_core::saves::SavesWarning;
use swofty_i18n::{tr, trf, trn};

use super::Shell;
use super::dialogs::saves_alert;

impl Shell {
    pub(crate) fn saves_warning(&self, id: &str, of: Option<SavesWarning>) -> Option<SavesWarning> {
        let careers =
            swofty_core::stdlib::careers::read(&swofty_core::stdlib::careers::data_path(&self.store.root));
        let name = self
            .library
            .addons
            .iter()
            .find(|a| a.manifest.id == id)
            .map_or_else(|| id.to_owned(), |a| a.manifest.name.clone());
        SavesWarning::with_careers(
            of,
            id,
            &name,
            &swofty_core::stdlib::careers::needing(&careers, id),
        )
    }

    pub(crate) fn turn_off_warning(&self, id: &str) -> Option<SavesWarning> {
        let of = self.library.registry.turn_off_warning(&self.library.profile, id);
        let on = self.library.profile.enabled().any(|e| e == id);
        match on {
            true => self.saves_warning(id, of),
            false => of,
        }
    }

    pub(crate) fn set_addon_enabled(
        &mut self,
        id: &str,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let registry = self.library.registry.clone();
        let warning = (!on).then(|| self.turn_off_warning(id)).flatten();
        let Some(warning) = warning else {
            self.update_profile(window, cx, |p| registry.set_enabled(p, id, on));
            return;
        };
        let shell = cx.entity().downgrade();
        let p = palette(cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, id) = (shell.clone(), warning.id.clone());
            saves_alert(alert, &warning, None, p)
                .ok_text(tr("Turn off anyway"))
                .on_ok(move |_, window, cx| {
                    shell
                        .update(cx, |s, cx| {
                            let registry = s.library.registry.clone();
                            s.update_profile(window, cx, |p| registry.set_enabled(p, &id, false));
                        })
                        .ok();
                    true
                })
        });
        cx.notify();
    }

    pub(crate) fn remove_addon(&mut self, id: &str, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let removes_plain = tr("Its files are deleted and it is taken out of every profile. \
                               You can install it again later.");
        let removes_db = tr(
            "Your DB Editor changes are deleted and it is taken out of every profile. \
                                  To keep them, use Share addon (.zip) in the DB Editor's Export menu first.",
        );
        let db_editor = self
            .library
            .addons
            .iter()
            .any(|a| a.manifest.id == id && a.source == crate::library::Source::DbEditor);
        let removes = if db_editor { removes_db } else { removes_plain };
        let shell = cx.entity().downgrade();
        let store = self.store.clone();
        let id = id.to_owned();
        let name = name.to_owned();
        let warning = self.saves_warning(
            &id,
            self.library
                .addons
                .iter()
                .find(|a| a.manifest.id == id)
                .and_then(|a| SavesWarning::of(&a.manifest)),
        );
        let p = palette(cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, store, id, name) = (shell.clone(), store.clone(), id.clone(), name.clone());
            let alert = match &warning {
                Some(w) => saves_alert(alert, w, Some(removes), p).ok_text(tr("Remove anyway")),
                None => alert
                    .title(trf!("Uninstall {name}?", name = name))
                    .description(removes)
                    .ok_text(tr("Uninstall")),
            };
            alert
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let (shell, store, id, name) = (shell.clone(), store.clone(), id.clone(), name.clone());
                    window
                        .spawn(cx, async move |cx| {
                            let removing = id.clone();
                            let removed = cx
                                .background_executor()
                                .spawn(async move { store.remove_addon(&removing) })
                                .await;
                            cx.update(|window, cx| match removed {
                                Ok(()) => {
                                    window.push_notification(
                                        Notification::success(trf!("Removed {name}", name = name)),
                                        cx,
                                    );
                                    shell
                                        .update(cx, |s, cx| {
                                            s.open_addon = None;
                                            s.reload_in_background(cx);
                                        })
                                        .ok();
                                }
                                Err(e) => {
                                    crate::app_log::action("remove addon").addon(&id).failed(&e);
                                    window.push_notification(Notification::error(e.to_string()), cx);
                                }
                            })
                            .ok();
                        })
                        .detach();
                    true
                })
        });
    }

    /// Takes `id` out of this profile; it stays installed for the others.
    pub(crate) fn remove_from_profile(
        &mut self,
        id: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keeps = tr(
            "It stays installed for your other profiles, and you can add it back \
                             with Add addons.",
        );
        let warning = self
            .library
            .profile
            .enabled()
            .any(|e| e == id)
            .then(|| self.turn_off_warning(id))
            .flatten();
        let Some(warning) = warning else {
            self.take_out_of_profile(id, name, window, cx);
            return;
        };
        let shell = cx.entity().downgrade();
        let (id, name) = (id.to_owned(), name.to_owned());
        let p = palette(cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, id, name) = (shell.clone(), id.clone(), name.clone());
            saves_alert(alert, &warning, Some(keeps), p)
                .ok_text(tr("Remove anyway"))
                .on_ok(move |_, window, cx| {
                    shell
                        .update(cx, |s, cx| s.take_out_of_profile(&id, &name, window, cx))
                        .ok();
                    true
                })
        });
    }

    pub(super) fn take_out_of_profile(
        &mut self,
        id: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let registry = self.library.registry.clone();
        let profile = self.library.profile.name.clone();
        let removed = self.store.update_profile(&profile, |p| {
            if p.enabled().any(|e| e == id) {
                registry.set_enabled(p, id, false)?;
            }
            p.remove_addon(id)
        });
        match removed {
            Ok(()) => {
                self.open_addon = None;
                window.push_notification(
                    Notification::success(trf!(
                        "Removed {name} from {profile}",
                        name = name,
                        profile = profile
                    )),
                    cx,
                );
            }
            Err(e) => {
                crate::app_log::action("remove addon from profile")
                    .addon(id)
                    .failed(&e);
                window.push_notification(Notification::error(e.to_string()), cx);
            }
        }
        self.reload(cx);
    }

    /// Picks installed addons this profile doesn't have and adds them, off.
    pub(crate) fn add_to_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut offered: Vec<(String, String, String)> = self
            .library
            .not_in_profile()
            .map(|m| {
                let man = &m.manifest;
                let detail = match man.author.trim() {
                    "" | "Unknown" => man.version.to_string(),
                    author => trf!("{version} · by {author}", version = man.version, author = author),
                };
                (man.id.clone(), man.name.clone(), detail)
            })
            .collect();
        offered.sort_by_cached_key(|(_, name, _)| name.to_lowercase());
        let profile = self.library.profile.name.clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Search installed addons")));
        let chosen: Rc<std::cell::RefCell<BTreeSet<String>>> = Rc::default();
        let shell = cx.entity().downgrade();
        let store = self.store.clone();
        let focus = std::cell::Cell::new(true);
        window.open_alert_dialog(cx, move |alert, window, cx| {
            if focus.replace(false) {
                search.update(cx, |s, cx| s.focus(window, cx));
            }
            let p = palette(cx);
            let query = search.read(cx).value().trim().to_lowercase();
            let rows: Vec<AnyElement> = offered
                .iter()
                .filter(|(id, name, _)| {
                    query.is_empty() || name.to_lowercase().contains(&query) || id.contains(&query)
                })
                .map(|(id, name, detail)| {
                    let on = chosen.borrow().contains(id);
                    let (chosen, id) = (chosen.clone(), id.clone());
                    h_flex()
                        .id(SharedString::from(format!("add-to-profile-{id}")))
                        .gap(px(10.))
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.hover))
                        .child(crate::addons::checkbox(on, p))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_size(px(13.)).text_color(p.text).truncate().child(name.clone()))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(p.text_muted)
                                        .truncate()
                                        .child(detail.clone()),
                                ),
                        )
                        .on_click(move |_, window, _| {
                            let mut chosen = chosen.borrow_mut();
                            if !chosen.remove(&id) {
                                chosen.insert(id.clone());
                            }
                            window.refresh();
                        })
                        .into_any_element()
                })
                .collect();
            let list = match rows.is_empty() {
                true => div()
                    .py(px(24.))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .text_center()
                    .child(if offered.is_empty() {
                        tr("Every installed addon is already in this profile")
                    } else {
                        tr("No installed addons match")
                    })
                    .into_any_element(),
                false => v_flex()
                    .max_h(px(360.))
                    .pr(px(SCROLLBAR_GAP))
                    .children(rows)
                    .scroll_y("add-to-profile-list")
                    .into_any_element(),
            };
            let picked: Vec<String> = offered
                .iter()
                .map(|(id, ..)| id)
                .filter(|id| chosen.borrow().contains(*id))
                .cloned()
                .collect();
            let (shell, store, profile) = (shell.clone(), store.clone(), profile.clone());
            let ok = match picked.len() {
                0 => tr("Add").to_owned(),
                n => trf!("Add {addons}", addons = trn(n, "{count} addon", "{count} addons")),
            };
            alert
                .width(px(520.))
                .title(trf!("Add addons to {profile}", profile = profile))
                .description(
                    tr("Installed addons this profile doesn't have. They're added off, at the end of the load order."),
                )
                .child(
                    v_flex()
                        .w_full()
                        .gap(px(10.))
                        .child(Input::new(&search).cleanable(true))
                        .child(list),
                )
                .ok_text(ok)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    if picked.is_empty() {
                        return true;
                    }
                    match store.update_profile(&profile, |p| picked.iter().try_for_each(|id| p.add_addon(id))) {
                        Ok(()) => {
                            let text = trf!(
                                "Added {addons} to {profile}",
                                addons = trn(picked.len(), "{count} addon", "{count} addons"),
                                profile = profile
                            );
                            window.push_notification(Notification::success(text), cx);
                        }
                        Err(e) => {
                            crate::app_log::action("add addons to profile").failed(&e);
                            window.push_notification(Notification::error(e.to_string()), cx);
                        }
                    }
                    shell.update(cx, |s, cx| s.reload(cx)).ok();
                    true
                })
        });
    }

    pub(crate) fn update_profile(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut swofty_core::profile::Profile) -> swofty_core::Result<()>,
    ) {
        if let Err(e) = self.store.update_profile(&self.library.profile.name, f) {
            crate::app_log::action("change the profile").failed(&e);
            window.push_notification(Notification::error(e.to_string()), cx);
        }
        self.reload(cx);
    }

    pub(crate) fn switch_profile(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.play_pending.cancel();
        if let Err(e) = self.store.set_active_profile(name) {
            crate::app_log::action("switch profile").failed(&e);
            window.push_notification(Notification::error(e.to_string()), cx);
        }
        self.reload_in_background(cx);
    }
}
