use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::shell::notices::Notification;
use gpui_kit::*;
use swofty_core::dbedit::draft::{Draft, Opened, Written};
use swofty_core::dbedit::package::{self, PackageInfo};
use swofty_i18n::{tr, trf};

use super::import::report_text;
use super::{Db, Load, Nav, Slots, Then, guarded, lock, short, text};
use crate::shell::host::Host;
use crate::shell::{Page, Shell};

const PERSIST_DELAY: Duration = Duration::from_millis(300);

impl Db {
    // Reads every enabled DB Editor addon off the UI thread for "Also changed by".
    pub(super) fn load_others<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (db, needs) = host.split();
        db.others_gen += 1;
        let at = db.others_gen;
        let (store, profile) = (needs.store.clone(), needs.profile.to_owned());
        cx.spawn(async move |this, cx| {
            let others = cx
                .background_executor()
                .spawn(async move {
                    guarded(|| package::enabled_changes(&store, &profile).map_err(text)).unwrap_or_default()
                })
                .await;
            this.update(cx, |host, cx| {
                let db = host.split().0;
                if db.others_gen == at {
                    db.others = Arc::new(others);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Writes the draft to disk off the UI thread once changes stop for a moment.
    pub(crate) fn persist<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let at = host.split().0.autosave.mark();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PERSIST_DELAY).await;
            this.update(cx, |host, cx| {
                let due = host.split().0.autosave.due(at);
                if due {
                    Self::flush(host, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Starts writing the draft's unwritten changes now.
    pub(crate) fn flush<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (db, needs) = host.split();
        if !db.autosave.begin() {
            return;
        }
        let (store, draft, file) = (needs.store.clone(), db.book.draft.clone(), db.file.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { guarded(|| lock(&file).write(&store, &draft).map_err(text)) })
                .await;
            this.update(cx, |host, cx| {
                let again = host.split().0.autosave.written(result);
                if again {
                    Self::persist(host, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Shell {
    pub(crate) fn db_count(&self) -> usize {
        self.db.book.draft.unsaved()
    }

    pub(crate) fn db_profile_changed(&mut self, cx: &mut Context<Self>) {
        self.db_refresh(cx);
    }

    pub(crate) fn db_locked(&self) -> bool {
        self.db.book.addon_error.is_some()
    }

    pub(crate) fn db_enabled(&self, id: &str) -> bool {
        self.library.profile.enabled().any(|e| e == id)
    }

    /// The DB Editor addons on in the active profile, in load order.
    pub(crate) fn db_stack(&self) -> Vec<String> {
        self.library
            .profile
            .enabled()
            .filter(|id| self.db.book.addon(id).is_some())
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn db_refresh(&mut self, cx: &mut Context<Self>) {
        let mut addons: Vec<PackageInfo> = self
            .library
            .addons
            .iter()
            .filter(|a| a.package)
            .map(|a| {
                let m = &a.manifest;
                PackageInfo {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    version: m.version.to_string(),
                    author: m.author.clone(),
                    description: m.description.clone().unwrap_or_default(),
                    changes: 0,
                    game_build: String::new(),
                }
            })
            .collect();
        addons.sort_by_key(|p| p.name.to_lowercase());
        self.db.book.addons = addons;
        if !self.db.draft_read {
            self.db_read_draft();
        }
        self.db_sync_addon(false, cx);
        Db::load_others(self, cx);
    }

    // Reads the autosaved draft once; a file that does not read is set aside.
    fn db_read_draft(&mut self) {
        self.db.draft_read = true;
        let store = self.store.clone();
        let opened = guarded(|| Ok(lock(&self.db.file).open(&store)));
        let body = match opened {
            Ok(Opened::Read(d)) => {
                self.db.book.draft = d;
                None
            }
            Ok(Opened::SetAside { error, kept }) => Some(trf!(
                "{error}\nIt was kept as {path}",
                error = error,
                path = kept.display()
            )),
            Ok(Opened::Stuck { error }) => {
                self.db.autosave.error = Some(trf!(
                    "Your changes are not saved to the draft until the damaged draft file is moved: {error}",
                    error = error
                ));
                Some(error)
            }
            Err(e) => Some(e),
        };
        if let Some(body) = body {
            self.db.toasts.push(
                Notification::error(body)
                    .title(tr("The DB Editor draft could not be read"))
                    .autohide(false),
            );
        }
        self.db.reindex();
    }

    /// Writes pending draft changes when the player leaves the DB Editor.
    pub(crate) fn db_page_change(&mut self, to: Page, cx: &mut Context<Self>) {
        if self.navigation.page == Page::Database && to != Page::Database {
            Db::flush(self, cx);
        }
        // Other pages change what choices offer, such as Driver Manager's drivers.
        if self.navigation.page != Page::Database && to == Page::Database {
            self.db.stale_choices = true;
            self.db.view_want = None;
            Db::refresh_view(self, cx);
        }
    }

    /// Writes the draft before the launcher quits.
    pub(crate) fn db_flush_now(&mut self) {
        if !self.db.autosave.unwritten || self.db.autosave.newer {
            return;
        }
        let (store, draft) = (self.store.clone(), &self.db.book.draft);
        if let Ok(Written::Saved) = guarded(|| lock(&self.db.file).write(&store, draft).map_err(text)) {
            self.db.autosave.unwritten = false;
        }
    }

    /// Replaces this window's draft with the newer one another window wrote.
    pub(crate) fn db_take_newer(&mut self, cx: &mut Context<Self>) {
        self.db.autosave.give_way();
        self.db.draft_read = false;
        self.db_read_draft();
        self.db.book.seen = None;
        self.db.errors.clear();
        self.db.cell = None;
        self.db.slots = Slots::default();
        self.db_sync_addon(true, cx);
        Db::changed(self, None, false, cx);
    }

    /// Keeps this window's draft, writing it over the one another window wrote.
    pub(crate) fn db_keep_mine(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        guarded(|| {
            lock(&self.db.file).adopt(&store);
            Ok(())
        })
        .ok();
        self.db.autosave.newer = false;
        Db::persist(self, cx);
        cx.notify();
    }

    // Another window wrote the draft while this one had nothing to write: take it.
    fn db_follow_draft(&mut self, cx: &mut Context<Self>) {
        if !self.db.autosave.idle() {
            return;
        }
        let store = self.store.clone();
        if guarded(|| Ok(lock(&self.db.file).is_newer(&store))).unwrap_or(false) {
            self.db_take_newer(cx);
            self.db.toasts.push(Notification::info(tr(
                "Another launcher window changed the draft, so it was reloaded.",
            )));
        }
    }

    // Follows the draft's addon on disk: removed, damaged, or saved by another writer.
    fn db_sync_addon(&mut self, quiet: bool, cx: &mut Context<Self>) {
        let Some(id) = self.db.book.draft.addon.clone() else {
            if self.db.book.addon_error.take().is_some() {
                Db::changed(self, None, false, cx);
            }
            return;
        };
        let store = self.store.clone();
        if !guarded(|| Ok(package::is_package(&store, &id))).unwrap_or(false) {
            let name = self.db.book.draft_name();
            if self.db.book.draft.is_dirty() {
                let edits = self.db.book.draft.edits.clone();
                self.db.book.draft.clear();
                self.db.book.draft.edits = edits;
                self.db.book.draft.name = Some(name.clone());
                self.db.toasts.push(Notification::warning(trf!(
                    "{name} was removed. Your unsaved changes stay in the DB Editor as a new addon.",
                    name = name
                )));
            } else {
                self.db.book.draft.clear();
            }
            self.db.book.addon_error = None;
            self.db.book.seen = None;
            Db::persist(self, cx);
            Db::changed(self, None, false, cx);
            return;
        }
        let stamp = guarded(|| Ok(package::modified(&store, &id))).ok().flatten();
        if self.db.book.seen == Some(stamp) {
            return;
        }
        self.db.book.seen = Some(stamp);
        if self.db.book.addon_error.is_none() && stamp == self.db.book.draft.stamp {
            return;
        }
        match guarded(|| package::Package::open(&store, &id).map_err(text)) {
            Ok(pkg) => {
                let reload = self.db.book.addon_error.is_none() && self.db.book.draft.stamp.is_some();
                let dirty = self.db.book.draft.is_dirty();
                self.db.book.draft.rebase(pkg.edits, stamp);
                self.db.book.addon_error = None;
                Db::persist(self, cx);
                if reload && !quiet {
                    let name = self.db.book.draft_name();
                    self.db.toasts.push(Notification::info(if dirty {
                        trf!(
                            "{name} changed on disk and was reloaded. Your unsaved changes are kept on top.",
                            name = name
                        )
                    } else {
                        trf!("{name} changed on disk and was reloaded.", name = name)
                    }));
                }
            }
            Err(e) => self.db.book.addon_error = Some(e),
        }
        self.db.errors.clear();
        self.db.cell = None;
        self.db.slots = Slots::default();
        Db::changed(self, None, false, cx);
    }

    /// Re-reads the draft's addon after this window changed its files, without a notice.
    pub(crate) fn db_quiet_reload(&mut self, cx: &mut Context<Self>) {
        self.db.book.seen = None;
        self.db_sync_addon(true, cx);
    }

    /// Try again on a draft whose addon did not read.
    pub(crate) fn db_retry(&mut self, cx: &mut Context<Self>) {
        self.db.book.seen = None;
        self.db_sync_addon(false, cx);
        cx.notify();
    }

    /// The poll's check: another writer (the CLI, a second window) changed the draft or its addon.
    pub(crate) fn db_poll(&mut self, cx: &mut Context<Self>) {
        if self.navigation.page != crate::shell::Page::Database {
            return;
        }
        self.db_follow_draft(cx);
        if self.db.book.draft.addon.is_some() {
            self.db_sync_addon(false, cx);
        }
    }

    pub(super) fn db_ready_to_edit(&mut self, cx: &mut Context<Self>) -> bool {
        self.db_sync_addon(false, cx);
        self.db.catalog().is_some() && self.db.book.addon_error.is_none()
    }

    pub(crate) fn db_turn_on(&mut self, id: &str) -> Result<(), String> {
        let registry = self.library.registry.clone();
        self.store
            .update_profile(&self.library.profile.name, |p| registry.set_enabled(p, id, true))
            .map_err(text)
    }

    /// Opens the DB Editor addon `id` as the draft. Unsaved changes in the
    /// draft ask Save / Discard / Cancel first.
    pub(crate) fn db_edit_addon(&mut self, id: &str, cx: &mut Context<Self>) {
        self.open_page(Page::Database, cx);
        if self.db.book.draft.addon.as_deref() == Some(id) {
            self.db_retry(cx);
        } else if self.db.book.draft.is_dirty() {
            self.db.pending_open = Some(id.to_owned());
        } else {
            self.db_then(Then::Open(id.to_owned()), cx);
        }
        cx.notify();
    }

    pub(crate) fn db_then(&mut self, then: Then, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let mut toast = None;
        match then {
            Then::Open(id) => {
                let draft = &mut self.db.book.draft;
                self.db.book.addon_error = guarded(|| draft.open(&store, &id).map_err(text)).err();
                if self.db.book.addon_error.is_some() {
                    self.db.book.draft.clear();
                    self.db.book.draft.addon = Some(id);
                }
            }
            Then::New => self.db.book.draft.clear(),
            Then::Load {
                file,
                name,
                edits,
                report,
            } => {
                self.db.book.draft.clear();
                self.db.book.draft.edits = edits;
                self.db.book.draft.name = Some(name);
                self.db.book.addon_error = None;
                let note = if report.format == "bin" && report.skipped.len() > 1 {
                    Notification::warning(report_text(&report)).autohide(false)
                } else {
                    Notification::success(report_text(&report))
                };
                toast = Some(note.title(trf!(
                    "Opened {file} as a new addon. Review it, then Save as Addon.",
                    file = file
                )));
                Db::go(self, Nav::Changes, cx);
            }
        }
        self.db.book.seen = Some(self.db.book.draft.stamp);
        self.db.errors.clear();
        self.db.cell = None;
        self.db.slots = Slots::default();
        Db::persist(self, cx);
        self.db.toasts.extend(toast);
        Db::load_others(self, cx);
        Db::changed(self, None, false, cx);
    }

    /// "Also changed by …" for a key another enabled DB Editor addon changes.
    pub(crate) fn db_others(&self, key: u64) -> Option<String> {
        let editing = self.db.book.draft.addon.as_deref();
        let others: Vec<&(String, String)> = self
            .db
            .others
            .get(&key)?
            .iter()
            .filter(|(id, _)| Some(id.as_str()) != editing)
            .collect();
        let last = others.last()?;
        let name = |id: &str| {
            self.db
                .book
                .addon(id)
                .map_or_else(|| id.to_owned(), |p| p.name.clone())
        };
        let list: Vec<String> = others
            .iter()
            .map(|(id, v)| format!("{} ({})", name(id), short(v)))
            .collect();
        let order: Vec<&str> = self.library.profile.enabled().collect();
        let at = |id: &str| order.iter().position(|o| *o == id);
        let mine = self.db.edit(key).is_some();
        let later = match editing {
            Some(id) => at(id).is_some_and(|i| at(&last.0).is_none_or(|j| i > j)),
            None => true,
        };
        let wins = if mine && later {
            Some(tr("your change wins").to_owned())
        } else if mine || others.len() > 1 {
            Some(trf!("{name} wins", name = name(&last.0)))
        } else {
            None
        };
        let mut text = trf!("Also changed by {names}", names = list.join(", "));
        if let Some(w) = wins {
            text.push_str(&format!(" · {w}"));
        }
        Some(text)
    }

    pub(crate) fn db_shared(&self, key: u64) -> bool {
        let editing = self.db.book.draft.addon.as_deref();
        self.db
            .others
            .get(&key)
            .is_some_and(|l| l.iter().any(|(id, _)| Some(id.as_str()) != editing))
    }

    // A new game folder or build: drop the catalog and everything that points into it.
    pub(super) fn db_forget_game(&mut self) {
        let db = &mut self.db;
        db.load = Load::Idle;
        db.game = None;
        db.view = None;
        db.view_want = None;
        db.rev += 1;
        db.row = None;
        db.key = None;
        db.reveal = None;
        db.cell = None;
        db.hits.clear();
        db.raw_rows = None;
        db.unnamed_rows = Arc::new(Vec::new());
        db.scopes.clear();
        db.slots = Slots::default();
        if matches!(db.nav, Nav::Raw(_)) {
            db.nav = Nav::Raw(0);
        }
    }
}

/// The draft and the DB Editor addons it can be saved into.
#[derive(Default)]
pub(crate) struct DraftBook {
    /// The changes being edited; `draft.edits` is what every view shows.
    pub draft: Draft,
    /// The DB Editor addons installed, from their manifests.
    pub addons: Vec<PackageInfo>,
    /// Why the draft's addon could not be read; editing stays off until it reads.
    pub addon_error: Option<String>,
    /// The addon file time last looked at, so a damaged file is read once per change.
    pub(super) seen: Option<Option<SystemTime>>,
}

impl DraftBook {
    pub(crate) fn addon(&self, id: &str) -> Option<&PackageInfo> {
        self.addons.iter().find(|p| p.id == id)
    }

    /// The installed addon the draft edits, if any.
    pub(crate) fn bound(&self) -> Option<&PackageInfo> {
        self.draft.addon.as_deref().and_then(|id| self.addon(id))
    }

    /// The draft's addon name, or "the new addon".
    pub(crate) fn draft_name(&self) -> String {
        match (&self.draft.addon, self.bound()) {
            (_, Some(info)) => info.name.clone(),
            (Some(id), None) => id.clone(),
            (None, None) => "the new addon".into(),
        }
    }

    /// A save wrote the addon: its last read error no longer holds, and its file is read again.
    pub(crate) fn saved(&mut self) {
        self.addon_error = None;
        self.seen = None;
    }
}
