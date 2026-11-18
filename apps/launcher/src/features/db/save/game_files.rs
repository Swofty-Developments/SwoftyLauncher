//! Exporting the draft as game files for players without Swofty Launcher.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::dbedit::package::{self, Package};
use swofty_i18n::{tr, trf, trn};

use crate::features::db::{Db, guarded, text};
use crate::shell::Shell;
use crate::shell::host::Host;
use crate::ui::Callout;
use crate::ui::theme::{Palette, palette};

pub(super) const GAME_FILES_NOTE: &str = "Players extract this .zip into the F1 25 folder. Steam's Verify integrity of game files undoes it, and Swofty Launcher won't start modded sessions while that file is replaced. Players who use Swofty Launcher should get the addon .zip instead.";

pub(super) fn choice_row(
    id: &'static str,
    on: bool,
    title: String,
    detail: String,
    enabled: bool,
    p: Palette,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .gap(px(10.))
        .px(px(12.))
        .py(px(9.))
        .rounded(px(6.))
        .border_1()
        .border_color(if on { p.accent } else { p.border_variant })
        .when(on, |d| d.bg(p.accent.opacity(0.08)))
        .when(enabled, |d| d.cursor_pointer())
        .when(!enabled, |d| d.opacity(0.5))
        .child(
            Icon::new(if on {
                IconName::CircleCheck
            } else {
                IconName::Circle
            })
            .size(px(15.))
            .text_color(if on { p.accent } else { p.text_placeholder }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(title),
                )
                .child(div().text_size(px(12.)).text_color(p.text_muted).child(detail)),
        )
}

/// What a game-files export applies.
#[derive(Clone)]
pub(super) enum Pick {
    Draft(Box<Package>),
    Addons(Vec<String>),
}

impl Shell {
    /// Builds a game database with the draft or the profile's DB Editor addons
    /// applied and saves it as a .zip of game files.
    pub(crate) fn db_game_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .db
            .book
            .bound()
            .map_or_else(|| tr("DB Editor changes").to_owned(), |i| i.name.clone());
        let n = self.db.book.draft.edits.len();
        let draft = (n > 0)
            .then(|| guarded(|| Package::unsaved(&name, self.db.book.draft.edits.clone()).map_err(text)).ok())
            .flatten();
        let profile = self.library.profile.name.clone();
        let stack = guarded(|| package::enabled_packages(&self.store, &profile).map_err(text))
            .unwrap_or_else(|_| self.db_stack());
        let every = Rc::new(Cell::new(draft.is_none()));
        let shell = cx.entity().downgrade();
        let p = palette(cx);
        let file_stem = self
            .db
            .book
            .draft
            .addon
            .as_deref()
            .map_or("db-editor", |id| id.trim_start_matches(package::ID_PREFIX))
            .to_owned();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let all = every.get();
            let (shell, stack2, draft2, every2) =
                (shell.clone(), stack.clone(), draft.clone(), every.clone());
            let single = draft.as_ref().map(|_| {
                let every = every.clone();
                choice_row(
                    "db-ge-one",
                    !all,
                    tr("The changes in the DB Editor").to_owned(),
                    trf!(
                        "{changes}, saved or not",
                        changes = trn(n, "{count} change", "{count} changes")
                    ),
                    true,
                    p,
                )
                .on_click(move |_, window, _| {
                    every.set(false);
                    window.refresh();
                })
            });
            let stacked = {
                let every = every.clone();
                let enabled = !stack.is_empty();
                choice_row(
                    "db-ge-all",
                    all,
                    trf!("Every DB Editor addon on in {profile}", profile = profile),
                    if enabled {
                        trf!(
                            "{addons} in load order",
                            addons = trn(stack.len(), "{count} addon", "{count} addons")
                        )
                    } else {
                        trf!("No DB Editor addons are on in {profile}", profile = profile)
                    },
                    enabled,
                    p,
                )
                .when(enabled, |d| {
                    d.on_click(move |_, window, _| {
                        every.set(true);
                        window.refresh();
                    })
                })
            };
            let ready = if all { !stack.is_empty() } else { draft.is_some() };
            let stem = file_stem.clone();
            alert
                .width(px(540.))
                .title(tr("Export game files"))
                .child(
                    v_flex()
                        .gap(px(10.))
                        .child(div().text_size(px(13.)).text_color(p.text_muted).child(tr(
                            "Saves a .zip with the game's database file, changed by the addons you pick.",
                        )))
                        .children(single)
                        .child(stacked)
                        .child(div().mt(px(4.)).child(Callout::warning(tr(GAME_FILES_NOTE)))),
                )
                .ok_text(tr("Export…"))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    if !ready {
                        return false;
                    }
                    let (pick, file) = match (every2.get(), &draft2) {
                        (false, Some(d)) => {
                            (Pick::Draft(Box::new(d.clone())), format!("{stem}-game-files.zip"))
                        }
                        _ => (
                            Pick::Addons(stack2.clone()),
                            "db-editor-addons-game-files.zip".to_owned(),
                        ),
                    };
                    shell
                        .update(cx, |s, cx| Db::export_game_files_to(s, pick, file, window, cx))
                        .ok();
                    true
                })
        });
    }
}

impl Db {
    fn export_game_files_to<H: Host<Self>>(
        host: &mut H,
        pick: Pick,
        file: String,
        window: &mut Window,
        cx: &mut Context<H>,
    ) {
        let needs = host.split().1;
        let store = needs.store.clone();
        let Some(game) = crate::features::sections::game(&store, needs.install) else {
            window.notify(
                Notification::error(tr("F1 25 was not found.")).title(tr("Export failed")),
                cx,
            );
            return;
        };
        let home = crate::features::db::home(&store);
        let run = move |out: PathBuf, window: &mut Window, cx: &mut Context<H>| {
            window.notify(Notification::info(tr("Building the game files…")), cx);
            let (store, game, pick) = (store.clone(), game.clone(), pick.clone());
            cx.spawn_in(window, async move |_, cx| {
                let target = out.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        guarded(|| {
                            match pick {
                                Pick::Draft(p) => package::export_packages(&store, &game, &[*p], &target),
                                Pick::Addons(ids) => package::export_game_files(&store, &game, &ids, &target),
                            }
                            .map_err(text)
                        })
                    })
                    .await;
                cx.update(|window, cx| match result {
                    Ok(done) => {
                        let mut body = trf!(
                            "{changes} · {size} · saved to {path}",
                            changes = trn(done.changes, "{count} change", "{count} changes"),
                            size = crate::ui::format::size(done.bytes),
                            path = out.display()
                        );
                        if !done.skipped.is_empty() {
                            body.push_str(&trf!("\n{count} skipped:", count = done.skipped.len()));
                            for line in done.skipped.iter().take(4) {
                                body.push_str(&format!("\n{line}"));
                            }
                        }
                        for line in &done.warnings {
                            body.push_str(&format!("\n{line}"));
                        }
                        let note = match done.warnings.is_empty() {
                            true => Notification::success(body),
                            false => Notification::warning(body).autohide(false),
                        };
                        window.notify(note.title(tr("Exported game files")), cx);
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
            run(dir.join(&file), window, cx);
            return;
        }
        let rx = cx.prompt_for_new_path(&home, Some(&file));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                this.update_in(cx, |_, window, cx| run(path, window, cx)).ok();
            }
        })
        .detach();
    }
}
