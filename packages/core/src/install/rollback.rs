//! Rolling a game folder back to its baseline from the journals of earlier installs, and which of them a
//! later build stranded.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::files::drop_partial;
use super::wine_override::remove_override;
use super::{Drift, Installer, RolledBack};
use crate::diagnostics::phases;
use crate::game_folder::baseline::Hashes;
use crate::game_folder::journal::{Journal, Op, State};
use crate::game_folder::placed::{self, Placed};
use crate::{Result, addons::storage as addon_storage, io_err, with_retry};

impl<'a> Installer<'a> {
    pub fn recover(&self) -> Result<Vec<Drift>> {
        Ok(self.roll_back(false)?.into_iter().flat_map(|r| r.drift).collect())
    }

    /// Roll back every open journal, or only the ones a `swofty run` session
    /// opened, oldest first.
    pub fn roll_back(&self, sessions_only: bool) -> Result<Vec<RolledBack>> {
        let mut hashes = phases::time("rollback.hashes_load", || Hashes::load(self.data));
        let out = self.roll_back_with(sessions_only, &mut hashes);
        phases::time("rollback.hashes_save", || hashes.save());
        out
    }

    pub(super) fn roll_back_with(&self, sessions_only: bool, hashes: &mut Hashes) -> Result<Vec<RolledBack>> {
        let mut out = Vec::new();
        for j in Journal::open(self.data)? {
            if sessions_only && !j.session {
                continue;
            }
            let (id, profile, session) = (j.id.clone(), j.profile.clone(), j.session);
            let changes = j.entries.len();
            let (drift, kept) = self.rollback(j, hashes)?;
            out.push(RolledBack {
                id,
                profile,
                session,
                changes,
                drift,
                kept,
            });
        }
        Ok(out)
    }

    /// A journal written for a game folder that no longer has the game, as
    /// after Steam moved the library or the game was reinstalled elsewhere.
    /// Only the files here that still hold exactly what it wrote are its to
    /// roll back; the rest stay and are reported.
    pub(super) fn stranded(&self, j: &Journal) -> bool {
        j.game.as_deref().is_some_and(|was| {
            !was.join(crate::GAME_EXE).is_file() && !crate::game_folder::copies::same_dir(was, &self.game.dir)
        })
    }

    pub(super) fn rollback(&self, j: Journal, hashes: &mut Hashes) -> Result<(Vec<Drift>, Vec<String>)> {
        let ids = addon_storage::ids(j.entries.iter().map(|e| e.path.as_str()));
        let started = std::time::Instant::now();
        let kept = self.keep_storage(&ids, std::slice::from_ref(&j))?;
        for id in &ids {
            addon_storage::save_current(self.data, id)?;
        }
        phases::record("rollback.storage", started.elapsed());
        phases::time("rollback.careers", || {
            crate::stdlib::careers::sync(self.data, Some(&self.game.dir), Some(&j.profile)).ok();
            if j.session {
                let log = std::fs::read_to_string(self.game.dir.join(crate::session::LOADER_LOG))
                    .unwrap_or_default();
                crate::stdlib::careers::record_session(self.data, &j.profile, &log, || {
                    crate::build::stock_max_ids(self.data, self.game)
                })
                .ok();
            }
        });
        let started = std::time::Instant::now();
        let mut report = Vec::new();
        let stranded = self.stranded(&j);
        let same_build = j.build_id == self.game.identity.build_id;
        let left = |path: &str| {
            if stranded {
                Drift::Stranded(path.to_owned())
            } else {
                Drift::LeftInPlace(path.to_owned())
            }
        };
        for e in j.entries.iter().rev() {
            match e.op {
                Op::ReplaceFile => {
                    let path = self.path(&e.path);
                    drop_partial(&path);
                    let now = hashes.sha256_opt(&path)?;
                    let before = e.before.as_deref().expect("replace-file has before");
                    if now.as_deref() == Some(before) {
                        continue;
                    }
                    // A game file that is gone was moved aside by the install and its replacement never took
                    // its name, as after a power cut; nothing of the player's is lost by putting it back.
                    let ours = (now == e.after && (same_build || !stranded))
                        || (!stranded && same_build && (e.state == State::Pending || now.is_none()));
                    if ours {
                        self.put_original(before, &path, hashes)?;
                    } else {
                        report.push(left(&e.path));
                    }
                }
                Op::CreateFile => {
                    let path = self.path(&e.path);
                    drop_partial(&path);
                    match hashes.sha256_opt(&path)? {
                        None => {}
                        Some(now)
                            if Some(&now) == e.after.as_ref()
                                || (!stranded
                                    && (e.state == State::Pending
                                        || e.path == crate::content::shaders::LIVE_FILE)) =>
                        {
                            with_retry(&path, || crate::remove_placed(&path))?;
                        }
                        Some(_) => report.push(left(&e.path)),
                    }
                }
                Op::CreateDir => {
                    let path = self.path(&e.path);
                    if path.is_dir() && std::fs::read_dir(&path).map_err(io_err(&path))?.next().is_none() {
                        with_retry(&path, || std::fs::remove_dir(&path))?;
                    }
                }
                Op::RegistryOverride => {
                    let path = PathBuf::from(&e.path);
                    let text = std::fs::read_to_string(&path).map_err(io_err(&path))?;
                    if let Some(out) = remove_override(&text) {
                        std::fs::write(&path, out).map_err(io_err(&path))?;
                    }
                }
            }
        }
        self.drop_originals(
            j.entries
                .iter()
                .filter(|e| e.op == Op::ReplaceFile)
                .filter_map(|e| e.before.as_deref()),
        );
        phases::record("rollback.files", started.elapsed());
        let started = std::time::Instant::now();
        let mut placed = Placed::load(self.data)?;
        for e in &j.entries {
            match e.op {
                Op::ReplaceFile | Op::CreateFile => placed.remove(&e.path),
                Op::RegistryOverride => placed.registry = None,
                Op::CreateDir => {}
            }
        }
        placed.save(self.data)?;
        j.close()?;
        phases::record("rollback.state", started.elapsed());
        Ok((report, kept))
    }

    /// Move what the addons `ids` wrote through the storage API out of their
    /// game folders to `addon-storage/` in the data folder: every file there
    /// that neither `placed.json` nor one of `open` records the launcher
    /// placing.
    pub(super) fn keep_storage(&self, ids: &BTreeSet<String>, open: &[Journal]) -> Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut managed: BTreeSet<String> = Placed::load(self.data)?.files.into_keys().collect();
        managed.extend(
            open.iter()
                .flat_map(|j| j.entries.iter().map(|e| placed::key(&e.path))),
        );
        let mut kept = Vec::new();
        for id in ids {
            kept.extend(addon_storage::keep(self.data, &self.game.dir, id, &managed)?);
        }
        Ok(kept)
    }
}
