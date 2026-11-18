use std::path::Path;

use crate::shell::notices::{self, Notification, Notify};
use gpui_kit::*;
use swofty_core::dbedit::ImportReport;
use swofty_core::dbedit::package::{self, Imported, Incoming};
use swofty_i18n::{tr, trf};

use super::{Then, debug_path, guarded, text};
use crate::features::db::{Db, DbChange, DbPrompt};
use crate::shell::Shell;
use crate::shell::host::PromptHost;

impl Db {
    pub(crate) fn import<H: PromptHost<Self>>(host: &mut H, window: &mut Window, cx: &mut Context<H>) {
        if let Some(path) = debug_path("SWOFTY_DEBUG_IMPORT") {
            Self::import_path(host, &path, window, cx);
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |host, window, cx| Self::import_path(host, &path, window, cx))
                .ok();
        })
        .detach();
    }

    fn import_path<H: PromptHost<Self>>(host: &mut H, path: &Path, window: &mut Window, cx: &mut Context<H>) {
        let file = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let (db, needs) = host.split();
        let Some(catalog) = db.catalog().cloned() else {
            return;
        };
        if db.importing {
            return;
        }
        db.importing = true;
        let (store, path) = (needs.store.clone(), path.to_path_buf());
        window.notify(Notification::info(trf!("Importing {file}…", file = file)), cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { guarded(|| package::read(&store, &catalog, &path).map_err(text)) })
                .await;
            this.update_in(cx, |host, window, cx| {
                host.split().0.importing = false;
                Self::imported(host, result, file, window, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // An addon .zip installs as a new addon and shows on Addons; other files open as a new draft.
    fn imported<H: PromptHost<Self>>(
        host: &mut H,
        result: Result<Incoming, String>,
        file: String,
        window: &mut Window,
        cx: &mut Context<H>,
    ) {
        match result {
            Ok(Incoming::Addon(done)) => host.changed(DbChange::Imported(done), cx),
            Ok(Incoming::Changes { name, edits, report }) => {
                let then = Then::Load {
                    file,
                    name,
                    edits,
                    report,
                };
                host.prompt(DbPrompt::Leave(then), window, cx);
            }
            Err(e) => {
                crate::app::app_log::action("db editor: import")
                    .file(std::path::Path::new(&file))
                    .failed(&e);
                window.notify(
                    Notification::error(e).title(trf!("Could not import {file}", file = file)),
                    cx,
                )
            }
        }
    }
}

impl Shell {
    /// Turns the imported addon on in this profile, says so, and shows it on Addons.
    pub(crate) fn db_addon_imported(&mut self, done: Imported, cx: &mut Context<Self>) {
        let id = done.package.id.clone();
        let profile = self.library.profile.name.clone();
        let on = !self.vanilla() && self.db_turn_on(&id).is_ok();
        let body = imported_text(&done, on.then_some(profile.as_str()));
        let title = trf!("Imported {name}", name = done.package.name);
        notices::post(Notification::success(body).title(title), cx);
        self.reload(cx);
        self.show_addon(&id, cx);
    }

    pub(crate) fn db_add_key(
        &mut self,
        path: &str,
        tag: u8,
        value: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let catalog = self.db.catalog().cloned().ok_or("The database is not loaded")?;
        if !self.db_ready_to_edit(cx) {
            return Err(self
                .db
                .book
                .addon_error
                .clone()
                .unwrap_or_else(|| "The draft can't be edited now.".into()));
        }
        let key = guarded(|| self.db.book.draft.edits.add(&catalog, path, tag, value))?;
        Db::changed(self, Some(key), true, cx);
        Ok(())
    }
}

/// The notice body for an imported addon, and the profile it was turned on in.
pub(super) fn imported_text(done: &Imported, turned_on_in: Option<&str>) -> String {
    let mut body = report_text(&done.report);
    if let Some(from) = &done.renamed_from {
        body.push_str(&trf!(
            "\n{from} was already installed; this copy is {id}",
            from = from,
            id = done.package.id
        ));
    }
    if let Some(profile) = turned_on_in {
        body.push_str(&trf!("\nTurned on in {profile}", profile = profile));
    }
    body
}

pub(super) fn report_text(r: &ImportReport) -> String {
    let format = match r.format {
        "ego" => tr("EGO GameDatabases Editor export"),
        "package" | "zip" => tr("DB Editor addon"),
        "bin" => tr("Modded game database"),
        _ => tr("DB Editor export"),
    };
    if r.format == "bin" {
        let mut lines = vec![format.to_owned(), trf!("{count} imported", count = r.imported)];
        lines.extend(r.skipped.iter().cloned());
        return lines.join("\n");
    }
    let mut lines = vec![
        format.to_owned(),
        trf!(
            "{imported} imported · {unchanged} unchanged",
            imported = r.imported,
            unchanged = r.unchanged
        ),
    ];
    if !r.skipped.is_empty() {
        lines.push(trf!("{count} skipped:", count = r.skipped.len()));
        lines.extend(r.skipped.iter().take(4).cloned());
        if r.skipped.len() > 4 {
            lines.push(trf!("and {count} more", count = r.skipped.len() - 4));
        }
    }
    lines.join("\n")
}
