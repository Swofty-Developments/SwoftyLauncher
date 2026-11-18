//! The draft addon's details: name, version, author, category and description.

use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::{Hit, px};
use gpui_kit::component::input::Input;
use gpui_kit::component::{WindowExt, h_flex, v_flex};
use gpui_kit::*;
use swofty_core::dbedit::package::Package;
use swofty_i18n::{tr, trf};

use crate::features::db::{guarded, text};
use crate::shell::Shell;
use crate::ui::Field;
use crate::ui::theme::palette;

use super::category_picker;

impl Shell {
    /// Name, version, author, category and description of the draft's addon.
    pub(crate) fn db_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.db.book.draft.addon.clone() else {
            return;
        };
        let pkg = match guarded(|| Package::open(&self.store, &id).map_err(text)) {
            Ok(p) => p,
            Err(e) => {
                crate::app::app_log::action("db editor: read the addon").failed(&e);
                window.notify(Notification::error(e).title(tr("Could not read the addon")), cx);
                return;
            }
        };
        let form = self.db_form(Some(&pkg), window, cx);
        let shell = cx.entity().downgrade();
        let p = palette(cx);
        let title = trf!("{name} details", name = pkg.manifest.name);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, fields, id) = (shell.clone(), form.clone(), id.clone());
            alert
                .width(px(560.))
                .title(title.clone())
                .child(
                    v_flex()
                        .gap(px(14.))
                        .child(Field::new(tr("NAME"), Input::new(&form.name).tap_sm()))
                        .child(
                            h_flex()
                                .gap(px(12.))
                                .child(
                                    div().w(px(110.)).children(
                                        form.version
                                            .as_ref()
                                            .map(|v| Field::new(tr("VERSION"), Input::new(v).tap_sm())),
                                    ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .child(Field::new(tr("AUTHOR"), Input::new(&form.author).tap_sm())),
                                )
                                .child(div().w(px(170.)).child(category_picker(form.category.clone()))),
                        )
                        .child(Field::new(
                            tr("DESCRIPTION"),
                            Input::new(&form.description).tap_sm(),
                        ))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_placeholder)
                                .child(trf!("ID {id} · versions look like 1.2.0", id = id)),
                        ),
                )
                .ok_text(tr("Save details"))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let [name, version, author, description] = fields.read(cx);
                    let category = fields.category.get();
                    let result = shell
                        .update(cx, |s, cx| {
                            let store = s.store.clone();
                            let r = guarded(|| {
                                let mut pkg = Package::open(&store, &id).map_err(text)?;
                                pkg.set_details(&name, &version, &author, &description)
                                    .map_err(text)?;
                                pkg.set_category(category);
                                pkg.write(&store).map_err(text)
                            });
                            if r.is_ok() {
                                if !author.is_empty() {
                                    s.db.book.draft.author = Some(author.clone());
                                }
                                s.db_quiet_reload(cx);
                                s.reload(cx);
                            }
                            r
                        })
                        .unwrap_or_else(|_| Err("The launcher is closing".into()));
                    match result {
                        Ok(()) => {
                            window.notify(Notification::success(trf!("Saved {name}", name = name)), cx);
                            true
                        }
                        Err(e) => {
                            crate::app::app_log::action("db editor: save the details").failed(&e);
                            window.notify(Notification::error(e).title(tr("Could not save the details")), cx);
                            false
                        }
                    }
                })
        });
    }
}
