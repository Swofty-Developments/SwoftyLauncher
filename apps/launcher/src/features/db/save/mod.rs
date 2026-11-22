use std::cell::Cell;
use std::rc::Rc;

use crate::shell::notices::{self, Notification, Notify};
use crate::ui::scale::{Hit, px};
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{WindowExt, h_flex, v_flex};
use gpui_kit::*;
use swofty_core::dbedit::draft::Details;
use swofty_core::dbedit::package::{self, Package};
use swofty_core::manifest::Category;
use swofty_core::store::Store;
use swofty_i18n::{tr, trf, trn};

use super::{Then, guarded, text};
use crate::features::db::{Db, DbChange};
use crate::shell::Shell;
use crate::shell::host::Host;
use crate::ui::Field;
use crate::ui::theme::palette;

mod details;
mod export;
mod game_files;
mod leave;
mod overrides;

#[cfg(test)]
pub(super) use overrides::{overrides_text, parts_text};

/// The author last typed, else one from an installed DB Editor addon; never
/// the OS user name.
pub(crate) fn default_author(store: &Store, last: Option<&str>) -> String {
    if let Some(a) = last.filter(|a| !a.trim().is_empty()) {
        return a.to_owned();
    }
    package::list(store)
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.author)
        .find(|a| a != package::DEFAULT_AUTHOR && !a.trim().is_empty())
        .unwrap_or_default()
}

/// The notice for a draft saved as a new addon, and the profile it was turned on in.
pub(super) fn saved_new_text(name: &str, turned_on_in: Option<&str>) -> String {
    match turned_on_in {
        Some(profile) => trf!(
            "Saved {name} and turned it on in {profile}",
            name = name,
            profile = profile
        ),
        None => trf!("Saved {name}", name = name),
    }
}

fn category_label(c: Option<Category>) -> &'static str {
    c.map_or(tr("Uncategorised"), Category::label)
}

fn category_picker(chosen: Rc<Cell<Option<Category>>>) -> Div {
    let current = chosen.get();
    v_flex().child(Field::new(
        tr("CATEGORY"),
        Button::new("db-save-category")
            .outline()
            .tap_sm()
            .w_full()
            .label(category_label(current))
            .dropdown_caret(true)
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                menu = menu.min_w(px(220.)).max_h(px(360.)).scrollable(true);
                let packages = Category::ALL.into_iter().filter(|c| *c != Category::Shaders);
                for c in packages.map(Some).chain([None]) {
                    let chosen = chosen.clone();
                    menu = menu.item(
                        PopupMenuItem::new(category_label(c))
                            .checked(c == current)
                            .on_click(move |_, window, _| {
                                chosen.set(c);
                                window.refresh();
                            }),
                    );
                }
                menu
            }),
    ))
}

#[derive(Clone)]
struct Form {
    name: Entity<InputState>,
    version: Option<Entity<InputState>>,
    author: Entity<InputState>,
    description: Entity<InputState>,
    category: Rc<Cell<Option<Category>>>,
}

impl Form {
    fn read(&self, cx: &App) -> [String; 4] {
        let v = |i: &Entity<InputState>| i.read(cx).value().trim().to_owned();
        [
            v(&self.name),
            self.version.as_ref().map(v).unwrap_or_default(),
            v(&self.author),
            v(&self.description),
        ]
    }
}

impl Shell {
    fn db_form(&self, existing: Option<&Package>, window: &mut Window, cx: &mut Context<Self>) -> Form {
        let mut input = |value: String, placeholder: &'static str, cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let m = existing.map(|p| &p.manifest);
        let name = m.map_or_else(
            || self.db.book.draft.name.clone().unwrap_or_default(),
            |m| m.name.clone(),
        );
        let author = m.map_or_else(
            || default_author(&self.store, self.db.book.draft.author.as_deref()),
            |m| m.author.clone(),
        );
        Form {
            name: input(name, tr("e.g. Faster pit stops"), cx),
            version: m.map(|m| input(m.version.to_string(), "1.0.0", cx)),
            author: input(author, tr("Your name"), cx),
            description: input(
                m.and_then(|m| m.description.clone()).unwrap_or_default(),
                tr("What the addon changes"),
                cx,
            ),
            category: Rc::new(Cell::new(m.and_then(|m| m.category))),
        }
    }

    /// The header's Save (an addon) or Save as Addon (a new draft).
    pub(crate) fn db_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.db_save_checked(None, window, cx);
    }

    pub(crate) fn db_save_as(&mut self, then: Option<Then>, window: &mut Window, cx: &mut Context<Self>) {
        if self.db.book.draft.edits.is_empty() {
            window.notify(
                Notification::info(tr("Change a value first, then save it as an addon.")),
                cx,
            );
            return;
        }
        let form = self.db_form(None, window, cx);
        let shell = cx.entity().downgrade();
        let p = palette(cx);
        let focus = Cell::new(true);
        let n = self.db.book.draft.edits.len();
        let note = if self.vanilla() {
            tr("Vanilla stays stock: turn the addon on in another profile to use it.").to_owned()
        } else {
            trf!(
                "{changes} {verb} an addon, turned on in {profile}.",
                changes = trn(n, "{count} change", "{count} changes"),
                verb = if n == 1 { tr("becomes") } else { tr("become") },
                profile = self.library.profile.name
            )
        };
        window.open_alert_dialog(cx, move |alert, window, cx| {
            if focus.replace(false) {
                form.name.update(cx, |s, cx| s.focus(window, cx));
            }
            let (shell, fields, then) = (shell.clone(), form.clone(), then.clone());
            alert
                .width(px(520.))
                .title(tr("Save as Addon"))
                .child(
                    v_flex()
                        .gap(px(14.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(p.text_muted)
                                .child(note.clone()),
                        )
                        .child(Field::new(tr("NAME"), Input::new(&form.name).tap_sm()))
                        .child(
                            h_flex()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .flex_1()
                                        .child(Field::new(tr("AUTHOR"), Input::new(&form.author).tap_sm())),
                                )
                                .child(div().w(px(190.)).child(category_picker(form.category.clone()))),
                        )
                        .child(Field::new(
                            tr("DESCRIPTION"),
                            Input::new(&form.description).tap_sm(),
                        )),
                )
                .ok_text(tr("Save as Addon"))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let [name, _, author, description] = fields.read(cx);
                    if name.is_empty() {
                        window.notify(Notification::error(tr("Give the addon a name.")), cx);
                        return false;
                    }
                    let details = Details {
                        name,
                        author,
                        category: fields.category.get(),
                        description,
                    };
                    shell
                        .update(cx, |s, cx| {
                            Db::save_new(s, details.clone(), then.clone(), window, cx)
                        })
                        .unwrap_or(false)
                })
        });
    }

    /// A new addon the draft was saved as: turned on in this profile, then what follows a save.
    pub(crate) fn db_saved_new(
        &mut self,
        id: String,
        details: &Details,
        then: Option<Then>,
        cx: &mut Context<Self>,
    ) {
        let profile = self.library.profile.name.clone();
        let on = !self.vanilla() && self.db_turn_on(&id).is_ok();
        let text = saved_new_text(&details.name, on.then_some(profile.as_str()));
        notices::post(Notification::success(text), cx);
        self.db_after_save(id, then, cx);
    }

    // A saved draft is cleared; the addon shows on Addons unless the save was on the way elsewhere.
    pub(crate) fn db_after_save(&mut self, id: String, then: Option<Then>, cx: &mut Context<Self>) {
        self.db.book.saved();
        Db::persist(self, cx);
        self.reload(cx);
        match then {
            Some(t) => self.db_then(t, cx),
            None => {
                Db::changed(self, None, false, cx);
                self.show_addon(&id, cx);
            }
        }
    }
}

impl Db {
    /// Saves the draft as a new addon. False when it could not be saved, which a notice says.
    pub(crate) fn save_new<H: Host<Self>>(
        host: &mut H,
        details: Details,
        then: Option<Then>,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> bool {
        let (db, needs) = host.split();
        let Some(catalog) = db.catalog().cloned() else {
            return false;
        };
        let store = needs.store;
        let draft = &mut db.book.draft;
        let pkg = match guarded(|| draft.save_as_addon(store, &catalog, &details).map_err(text)) {
            Ok(p) => p,
            Err(e) => {
                crate::app::app_log::action("db editor: save the addon").failed(&e);
                window.notify(Notification::error(e).title(tr("Could not save the addon")), cx);
                return false;
            }
        };
        let saved = DbChange::SavedNew {
            id: pkg.id,
            details,
            then,
        };
        host.changed(saved, cx);
        true
    }

    /// Saves the draft into the addon it edits. False when it could not be saved, which a notice says.
    pub(crate) fn save_bound<H: Host<Self>>(
        host: &mut H,
        then: Option<Then>,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> bool {
        let (db, needs) = host.split();
        let Some(catalog) = db.catalog().cloned() else {
            return false;
        };
        if let Some(e) = db.book.addon_error.clone() {
            crate::app::app_log::action("db editor: save").failed(&e);
            window.notify(Notification::error(e).title(tr("Could not save")), cx);
            return false;
        }
        let name = db.book.draft_name();
        let store = needs.store;
        let draft = &mut db.book.draft;
        match guarded(|| draft.save_addon(store, &catalog).map_err(text)) {
            Ok(pkg) => {
                window.notify(Notification::success(trf!("Saved {name}", name = name)), cx);
                host.changed(DbChange::Saved { id: pkg.id, then }, cx);
                true
            }
            Err(e) => {
                crate::app::app_log::action("db editor: save").failed(&e);
                window.notify(
                    Notification::error(e).title(trf!("Could not save {name}", name = name)),
                    cx,
                );
                false
            }
        }
    }
}
