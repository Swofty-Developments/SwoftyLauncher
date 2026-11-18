//! Sharing the draft as an addon .zip.

use std::path::PathBuf;

use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::Disableable;
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::*;
use swofty_core::dbedit::package::{self, Package};
use swofty_i18n::{tr, trf, trn};

use crate::features::db::{Db, guarded, text};
use crate::shell::Shell;
use crate::shell::host::Host;

impl Shell {
    /// "Share addon (.zip)" and "Export game files (.zip)".
    pub(crate) fn db_export_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let shell = cx.entity().downgrade();
        let share = self.db.book.draft.addon.is_some() && !self.db_locked();
        let game = !self.db.book.draft.edits.is_empty() || !self.db_stack().is_empty();
        Button::new("db-export")
            .outline()
            .icon(IconName::FileUp)
            .label(tr("Export"))
            .dropdown_caret(true)
            .disabled(!(share || game) || self.db.catalog().is_none())
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let (s1, s2) = (shell.clone(), shell.clone());
                menu.min_w(px(240.))
                    .item(
                        PopupMenuItem::new(tr("Share addon (.zip)…"))
                            .icon(IconName::Package)
                            .disabled(!share)
                            .on_click(move |_, window, cx| {
                                s1.update(cx, |s, cx| Db::export_addon(s, window, cx)).ok();
                            }),
                    )
                    .item(
                        PopupMenuItem::new(tr("Export game files (.zip)…"))
                            .icon(IconName::FileArchive)
                            .disabled(!game)
                            .on_click(move |_, window, cx| {
                                s2.update(cx, |s, cx| s.db_game_export(window, cx)).ok();
                            }),
                    )
            })
    }
}

impl Db {
    /// Shares the draft's addon as a .zip.
    pub(crate) fn export_addon<H: Host<Self>>(host: &mut H, window: &mut Window, cx: &mut Context<H>) {
        let (db, needs) = host.split();
        let Some(id) = db.book.draft.addon.clone() else {
            return;
        };
        let name = db.book.draft_name();
        let unsaved = db.book.draft.unsaved();
        let file = format!("{}.zip", id.trim_start_matches(package::ID_PREFIX));
        let store = needs.store.clone();
        let home = crate::features::db::home(&store);
        let export = move |path: PathBuf, window: &mut Window, cx: &mut App| {
            let (store, id) = (store.clone(), id.clone());
            let target = path.clone();
            let task = cx
                .background_executor()
                .spawn(async move { guarded(|| Package::export_zip(&store, &id, &target).map_err(text)) });
            let name = name.clone();
            window
                .spawn(cx, async move |cx| {
                    let result = task.await;
                    cx.update(|window, cx| match result {
                        Ok(()) => {
                            let mut body = trf!("Saved to {path}", path = path.display());
                            if unsaved > 0 {
                                body.push_str(&trf!(
                                    "\n{changes} aren't in it; save, then export again to include them.",
                                    changes =
                                        trn(unsaved, "{count} unsaved change", "{count} unsaved changes")
                                ));
                            }
                            window.notify(
                                Notification::success(body).title(trf!("Exported {name}", name = name)),
                                cx,
                            );
                        }
                        Err(e) => {
                            crate::app::app_log::action("db editor: export").failed(&e);
                            window.notify(Notification::error(e).title(tr("Export failed")), cx)
                        }
                    })
                    .ok();
                })
                .detach();
        };
        if let Some(dir) = crate::features::db::debug_path("SWOFTY_DEBUG_EXPORT") {
            export(dir.join(&file), window, cx);
            return;
        }
        let rx = cx.prompt_for_new_path(&home, Some(&file));
        cx.spawn_in(window, async move |_, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                cx.update(|window, cx| export(path, window, cx)).ok();
            }
        })
        .detach();
    }
}
