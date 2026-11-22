//! The checks before an install changes the game folder: drop-ins it may not replace, missing
//! originals it puts back, replaced files against their recorded originals, and files in the way.

use std::collections::BTreeMap;

use super::files::describe_file;
use super::{Conflict, Installer, Plan};
use crate::game_folder::baseline::{Baseline, FileRecord, Hashes, Objects, sha256_file};
use crate::game_folder::placed::Placed;
use crate::{Error, Result, io_err};

/// Refuses a plan that would replace an upscaler drop-in the player put in
/// the game folder.
pub(super) fn refuse_drop_ins(plan: &Plan) -> Result<()> {
    match plan
        .data
        .keys()
        .chain(plan.added.keys())
        .chain(plan.loader.keys())
        .find(|rel| crate::game_folder::stock::is_upscaler_drop_in(rel))
    {
        Some(rel) => Err(Error::Resolve(format!(
            "{rel} is an upscaler drop-in file and stays as the player put it; no addon or profile can replace it"
        ))),
        None => Ok(()),
    }
}

/// The hash of each file `plan` places, by game path.
pub(super) fn hashed<'p>(plan: &'p Plan, hashes: &mut Hashes) -> Result<BTreeMap<&'p str, String>> {
    let mut after = BTreeMap::new();
    for (rel, src) in plan.data.iter().chain(&plan.added).chain(&plan.loader) {
        after.insert(rel.as_str(), hashes.sha256(src)?);
    }
    Ok(after)
}

impl<'a> Installer<'a> {
    /// Whether the file at `rel`, which the baseline has no original for, may
    /// be recorded as one. Never when it holds exactly what the launcher wrote
    /// there in place of an original (a session whose journal and baseline
    /// were both lost); and while a damaged baseline is being rebuilt, only
    /// when its bytes are already a backed-up original.
    pub(super) fn may_record(
        &self,
        baseline: &Baseline,
        objects: &Objects,
        placed: &Placed,
        rel: &str,
    ) -> Result<bool> {
        let ours = placed.get(rel).filter(|p| p.replaced);
        if ours.is_none() && !baseline.checking() {
            return Ok(true);
        }
        let now = sha256_file(&self.path(rel))?;
        if ours.is_some_and(|p| p.sha256 == now) {
            return Ok(false);
        }
        Ok(!baseline.checking() || objects.has(&now))
    }

    /// Puts back each game file `plan` replaces that is missing from the game
    /// folder from its backed-up original. Refuses the plan, before putting
    /// any back, when one of them has none.
    pub(super) fn put_back_missing(&self, plan: &Plan, baseline: &Baseline, objects: &Objects) -> Result<()> {
        let (mut backed_up, mut missing) = (Vec::new(), Vec::new());
        for rel in plan.data.keys() {
            if std::fs::symlink_metadata(self.path(rel)).is_ok() {
                continue;
            }
            match baseline.files.get(rel).filter(|r| objects.has(&r.sha256)) {
                Some(rec) => backed_up.push((rel, rec.sha256.clone())),
                None => missing.push(rel.clone()),
            }
        }
        if !missing.is_empty() {
            return Err(Error::MissingStock {
                files: missing,
                version: crate::game_folder::exe_version::read(&self.path(crate::GAME_EXE)),
            });
        }
        for (rel, sha) in backed_up {
            self.restore_original(&sha, &self.path(rel))?;
            (self.out)(&format!(
                "{rel} was missing from the game folder; put back its backed-up original"
            ));
        }
        Ok(())
    }

    /// Checks each game file `plan` replaces against its recorded original,
    /// recording it when it has none and re-recording one Steam proves the
    /// record stale. Returns the files in the way and the ones with no
    /// original that may be recorded; stops at a file that is not Steam's.
    pub(super) fn check_replacements(
        &self,
        plan: &Plan,
        hashes: &mut Hashes,
        baseline: &mut Baseline,
        objects: &Objects,
        placed: &Placed,
        after: &BTreeMap<&str, String>,
    ) -> Result<(Vec<Conflict>, Vec<String>)> {
        let mut conflicts = Vec::new();
        let mut unproven = Vec::new();
        for rel in plan.data.keys() {
            let path = self.path(rel);
            if !baseline.files.contains_key(rel)
                && let Some(left) = placed.get(rel).filter(|p| !p.replaced)
                && hashes.sha256_opt(&path)?.as_deref() == Some(left.sha256.as_str())
            {
                conflicts.push(Conflict {
                    path: rel.clone(),
                    sha256: Some(left.sha256.clone()),
                    owned: true,
                    replaced: false,
                });
                continue;
            }
            if !baseline.files.contains_key(rel) && !self.may_record(baseline, objects, placed, rel)? {
                unproven.push(rel.clone());
                continue;
            }
            let rec = baseline.record(objects, &self.game.dir, rel, self.depot)?.clone();
            let now = hashes.sha256(&path)?;
            if now != rec.sha256 {
                let owned = placed.wrote(rel, &now) || after[rel.as_str()] == now;
                if !owned {
                    match self.depot.stock(rel, &path)? {
                        Some(true) => {
                            let size = std::fs::metadata(&path).map_err(io_err(&path))?.len();
                            let sha256 = objects.put_file(&path)?;
                            baseline.files.insert(
                                rel.clone(),
                                FileRecord {
                                    size,
                                    sha256,
                                    source: crate::game_folder::baseline::Source::Depot,
                                },
                            );
                            (self.out)(&format!(
                                "{rel} matches Steam's copy; its recorded original was out of date and has been replaced"
                            ));
                            continue;
                        }
                        Some(false) => {
                            (self.out)(&format!(
                                "{rel} is not Steam's copy and swofty did not write it: {}",
                                describe_file(&path, &now)
                            ));
                            return Err(Error::NotStock(vec![rel.clone()]));
                        }
                        None => (self.out)(&format!(
                            "{rel} differs from its recorded original and cannot be checked against Steam's list: {}",
                            describe_file(&path, &now)
                        )),
                    }
                }
                conflicts.push(Conflict {
                    owned,
                    path: rel.clone(),
                    sha256: Some(now),
                    replaced: true,
                });
            }
        }
        Ok((conflicts, unproven))
    }

    /// Refuses the plan when a file it replaces has no original that may be
    /// recorded, after saving the baseline so a rebuild tells the player to
    /// verify the game files.
    pub(super) fn check_unproven(&self, baseline: &mut Baseline, unproven: &[String]) -> Result<()> {
        if unproven.is_empty() {
            return Ok(());
        }
        let rebuilt = baseline.rebuild.is_some();
        if let Some(r) = baseline.rebuild.as_mut() {
            r.told_to_verify = true;
        }
        baseline.save(self.data)?;
        let first = &unproven[0];
        let more = match unproven.len() {
            1 => String::new(),
            n => format!(" (and {} more)", n - 1),
        };
        Err(Error::Other(if rebuilt {
            format!(
                "{}. {first}{more} could not be checked against a backed-up original; nothing in the game folder was changed",
                crate::game_folder::baseline::DAMAGED
            )
        } else {
            format!(
                "{}. {first}{more} still holds a file swofty wrote, and no original is on record for it; nothing in the game folder was changed",
                crate::game_folder::baseline::DAMAGED
            )
        }))
    }

    /// The files `plan` adds that are already in the game folder, each owned
    /// when it holds what the launcher wrote or is about to write there.
    pub(super) fn check_new_files(
        &self,
        plan: &Plan,
        hashes: &mut Hashes,
        baseline: &Baseline,
        placed: &Placed,
        after: &BTreeMap<&str, String>,
    ) -> Result<Vec<Conflict>> {
        let mut conflicts = Vec::new();
        for rel in plan.added.keys().chain(plan.loader.keys()) {
            let path = self.path(rel);
            if std::fs::symlink_metadata(&path).is_err() {
                continue;
            }
            let now = hashes.sha256_opt(&path)?;
            let stock = baseline.files.contains_key(rel);
            conflicts.push(Conflict {
                owned: now
                    .as_deref()
                    .is_some_and(|now| placed.wrote(rel, now) || (!stock && after[rel.as_str()] == now)),
                path: rel.clone(),
                sha256: now,
                replaced: false,
            });
        }
        Ok(conflicts)
    }
}
